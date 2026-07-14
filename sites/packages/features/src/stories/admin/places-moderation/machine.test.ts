import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  moderateMachine,
  MODERATE_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveModerateSnapshot,
  slugToState,
  stateToSlug,
  simulateModerateDecision,
  type ModerateFn,
  type TrackFn,
} from "./machine";
import type {
  ModerationResult,
  ReportRow,
} from "@data/lib/catalyst/admin/places-moderation";

const REPORTS: ReportRow[] = [
  {
    id: "1042",
    entity_id: "place-abc",
    reporter: "0xreporter1",
    status: "open",
    reason: "scam_or_spam",
    resolution: null,
    notes: null,
    resolved_by: null,
    resolved_at: null,
    created_at: "2026-06-21T14:32:11.000Z",
    place_title: "Rishi's Palace",
    place_coords: "22,-75",
    place_image: null,
    place_creator: "Dhingia Builds",
    payload: null,
  },
  {
    id: "992",
    entity_id: "place-xyz",
    reporter: "0xreporter2",
    status: "resolved",
    reason: "other",
    resolution: "no_violation",
    notes: "closed",
    resolved_by: "moderator",
    resolved_at: "2026-06-20T11:02:17.000Z",
    created_at: "2026-06-19T16:20:00.000Z",
    place_title: "Dollhouse",
    place_coords: "-103,-97",
    place_image: null,
    place_creator: "SDK",
    payload: null,
  },
];

const RESULT: ModerationResult = {
  report: { ...REPORTS[0]!, status: "resolved", resolved_at: "now" },
  placeDisabled: false,
  reportBody: { status: "resolved", resolved_by: "moderator" },
};

const okModerate: ModerateFn = async () => RESULT;

function inputFor(moderate: ModerateFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "admin-places-moderation",
      variant: "bucketed_queue",
      experimentKey: "admin_place_moderation_queue",
    },
    reports: REPORTS,
    queueOpenCount: 1,
    queueTotal: 2,
    moderate,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "queue",
  "reviewReport",
  "decision",
  "submitting",
  "moderated",
]);

describe("moderateMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(moderateMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);
  });

  it("slugs are unique and round-trip via SLUG_TO_STATE", () => {
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.queue);
    expect(slugToState(null)).toBe("queue");
    expect(slugToState(undefined)).toBe("queue");
    expect(slugToState("")).toBe("queue");
    expect(slugToState("nope")).toBe("queue");
    expect(slugToState("queue")).toBe("queue");
    expect(slugToState("review-report")).toBe("reviewReport");
    expect(slugToState("submitting")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("moderateMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveModerateSnapshot({
      step: "queue",
      trackCtx: inputFor(okModerate, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("boots straight into queue \u{2014} there is no client-side gate to pass", () => {
    const track = vi.fn();
    const snapshot = resolveModerateSnapshot({
      step: "queue",
      trackCtx: inputFor(okModerate, track).trackCtx,
      reports: REPORTS,
      queueOpenCount: 1,
      queueTotal: 2,
      track,
    });
    const actor = createActor(moderateMachine, {
      input: inputFor(okModerate, track),
      snapshot,
    }).start();

    // The wizard is only mounted when the loader's server-side read came back
    // ok (catalyrst-places/src/handlers/admin.rs:41 -> auth.rs:88-100), so the
    // queue is a genuine first view and reports itself as one.
    expect(actor.getSnapshot().matches("queue")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toEqual([
      MODERATE_EVENTS.queueViewed,
    ]);
  });

  it("hydrating submitting does NOT auto-commit and fires no telemetry", async () => {
    const track = vi.fn();
    const moderate = vi.fn(okModerate);
    const snapshot = resolveModerateSnapshot({
      step: "submitting",
      trackCtx: inputFor(moderate, track).trackCtx,
      reports: REPORTS,
      reportId: "1042",
      decision: "action",
      moderate,
      track,
    });
    const actor = createActor(moderateMachine, {
      input: inputFor(moderate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.decision).toBe("action");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(moderate).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveModerateSnapshot({
      step: "reviewReport",
      trackCtx: inputFor(okModerate, track).trackCtx,
      reports: REPORTS,
      reportId: "1042",
      track,
    });
    const actor = createActor(moderateMachine, {
      input: inputFor(okModerate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("reviewReport")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "DECIDE", decision: "dismiss" });
    expect(actor.getSnapshot().matches("decision")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(MODERATE_EVENTS.decisionSelected);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "OPEN" as const, reportId: "1042" },
  { type: "CLOSE" as const },
  { type: "DECIDE" as const, decision: "resolve" as const },
  { type: "TOGGLE_DISABLE" as const, disabled: true },
  { type: "CANCEL" as const },
  { type: "CONFIRM" as const },
  { type: "CONTINUE" as const },
];

describe("moderateMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(moderateMachine, {
      input: inputFor(okModerate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("queue")).toBe(true);
    expect(ends.has("reviewReport")).toBe(true);
    expect(ends.has("decision")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching decision passes through OPEN and DECIDE \u{2014} never a client gate", () => {
    const paths = getShortestPaths(moderateMachine, {
      input: inputFor(okModerate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const decision = paths.find((p) => (p.state.value as string) === "decision");
    expect(decision).toBeDefined();
    const events = decision!.steps.map((s) => s.event.type);
    expect(events).not.toContain("SIGN_IN");
    expect(events).toContain("OPEN");
    expect(events).toContain("DECIDE");
  });
});

describe("moderateMachine \u{2014} telemetry events (happy path)", () => {
  it("open -> decide -> confirm -> moderated fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(moderateMachine, {
      input: inputFor(okModerate, track),
    }).start();

    expect(actor.getSnapshot().matches("queue")).toBe(true);

    actor.send({ type: "OPEN", reportId: "1042" });
    expect(actor.getSnapshot().matches("reviewReport")).toBe(true);

    actor.send({ type: "DECIDE", decision: "resolve", resolution: "no_violation" });
    expect(actor.getSnapshot().matches("decision")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("moderated"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(MODERATE_EVENTS.queueViewed);
    expect(events).toContain(MODERATE_EVENTS.reportOpened);
    expect(events).toContain(MODERATE_EVENTS.decisionSelected);
    expect(events).toContain(MODERATE_EVENTS.committed);

    expect(events.indexOf(MODERATE_EVENTS.reportOpened)).toBeLessThan(
      events.indexOf(MODERATE_EVENTS.committed),
    );

    const openedCall = track.mock.calls.find((c) => c[0] === MODERATE_EVENTS.reportOpened);
    expect(openedCall?.[1]).toMatchObject({ report_id: "1042", entity_id: "place-abc" });

    const committedCall = track.mock.calls.find((c) => c[0] === MODERATE_EVENTS.committed);
    expect(committedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "admin_place_moderation_queue",
      variant: "bucketed_queue",
    });
  });

  it("action decision defaults disablePlace true and toggle fires disable_toggled", () => {
    const track = vi.fn();
    const actor = createActor(moderateMachine, {
      input: inputFor(okModerate, track),
    }).start();

    actor.send({ type: "OPEN", reportId: "1042" });
    actor.send({ type: "DECIDE", decision: "action" });
    expect(actor.getSnapshot().context.disablePlace).toBe(true);

    actor.send({ type: "TOGGLE_DISABLE", disabled: false });
    expect(actor.getSnapshot().context.disablePlace).toBe(false);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(MODERATE_EVENTS.disableToggled);
    const toggleCall = track.mock.calls.find((c) => c[0] === MODERATE_EVENTS.disableToggled);
    expect(toggleCall?.[1]).toMatchObject({ place_id: "place-abc", disabled: false });
  });
});

describe("moderateMachine \u{2014} commit failure + retry", () => {
  it("commit error -> decision (with error) -> CONFIRM recovers to moderated", async () => {
    const track = vi.fn();
    let calls = 0;
    const moderate: ModerateFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("catalyst unreachable");
      return okModerate(args);
    };

    const actor = createActor(moderateMachine, {
      input: inputFor(moderate, track),
    }).start();

    actor.send({ type: "OPEN", reportId: "1042" });
    actor.send({ type: "DECIDE", decision: "dismiss" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("decision") && s.context.error !== undefined);
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");

    const failEvents = track.mock.calls.map((c) => c[0]);
    expect(failEvents).toContain(MODERATE_EVENTS.failed);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("moderated"));
    expect(track.mock.calls.map((c) => c[0])).toContain(MODERATE_EVENTS.committed);
  });
});

describe("simulateModerateDecision", () => {
  it("resolves a patched report (status flipped, resolved_at stamped); no network", async () => {
    const res = await simulateModerateDecision({
      report: REPORTS[0]!,
      decision: "action",
      resolution: "content_removed",
      notes: "disabled pending appeal",
      disablePlace: true,
    });
    expect(res.report.status).toBe("actioned");
    expect(res.report.resolved_at).toBeTruthy();
    expect(res.placeDisabled).toBe(true);
    expect(res.reportBody.status).toBe("actioned");
    expect(res.disableBody).toMatchObject({ disabled: true });
  });

  it("reopen clears resolution/resolved_at and sends no disable body", async () => {
    const res = await simulateModerateDecision({
      report: REPORTS[1]!,
      decision: "reopen",
    });
    expect(res.report.status).toBe("open");
    expect(res.report.resolved_at).toBeNull();
    expect(res.report.resolution).toBeNull();
    expect(res.disableBody).toBeUndefined();
  });
});
