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
  simulateModerateAction,
  failClosedModerateAction,
  type ModerateFn,
  type TrackFn,
} from "./machine";
import type { SimulatedModeration } from "@data/lib/catalyst/admin/whatson-admin";

const RESULT: SimulatedModeration = {
  simulated: true,
  id: "evt-1",
  local: { approved: true, rejected: false },
};

const okModerate: ModerateFn = async ({ eventId }) => ({ ...RESULT, id: eventId });
const failModerate: ModerateFn = async () => {
  throw new Error("catalyst unreachable");
};

function inputFor(moderate: ModerateFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "landings-whatson-admin-moderate",
      variant: "moderation_wizard",
      experimentKey: "lp_whatson_admin_moderation",
    },
    moderate,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "authGate",
  "queue",
  "reviewEvent",
  "decision",
  "submitting",
  "moderated",
]);

const TRAVERSAL_EVENTS = [
  { type: "SIGN_IN" as const },
  { type: "OPEN" as const, eventId: "evt-1" },
  { type: "DECIDE" as const, action: "approve" as const },
  { type: "CANCEL" as const },
  { type: "CLOSE" as const },
  { type: "CONFIRM" as const },
  { type: "RETRY" as const },
  { type: "CONTINUE" as const },
];

describe("moderateMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(moderateMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);
  });

  it("slugs are the audit-spec step ids, unique, and round-trip", () => {
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    expect(slugs).toEqual([
      "auth-gate",
      "queue",
      "review-event",
      "decision",
      "submitting",
      "moderated",
    ]);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.authGate);
    expect(slugToState(null)).toBe("authGate");
    expect(slugToState(undefined)).toBe("authGate");
    expect(slugToState("")).toBe("authGate");
    expect(slugToState("nope")).toBe("authGate");
    expect(slugToState("review-event")).toBe("reviewEvent");
    expect(slugToState("decision")).toBe("decision");
    expect(slugToState("submitting")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("moderateMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveModerateSnapshot({
      step: "authGate",
      trackCtx: inputFor(okModerate, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-moderate", async () => {
    const track = vi.fn();
    const moderate = vi.fn(okModerate);
    const snapshot = resolveModerateSnapshot({
      step: "submitting",
      trackCtx: inputFor(moderate, track).trackCtx,
      moderate,
      track,
      eventId: "evt-7",
      action: "approve",
    });
    const actor = createActor(moderateMachine, {
      input: inputFor(moderate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.eventId).toBe("evt-7");
    expect(actor.getSnapshot().context.action).toBe("approve");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(moderate).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("hydrating queue does NOT re-fire the queue_viewed entry action", () => {
    const track = vi.fn();
    const snapshot = resolveModerateSnapshot({
      step: "queue",
      trackCtx: inputFor(okModerate, track).trackCtx,
      track,
    });
    const actor = createActor(moderateMachine, {
      input: inputFor(okModerate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("queue")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveModerateSnapshot({
      step: "reviewEvent",
      trackCtx: inputFor(okModerate, track).trackCtx,
      track,
      eventId: "evt-3",
    });
    const actor = createActor(moderateMachine, {
      input: inputFor(okModerate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("reviewEvent")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "DECIDE", action: "reject", rejectReasons: ["invalid_image"] });
    expect(actor.getSnapshot().matches("decision")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(MODERATE_EVENTS.decisionMade);
  });
});

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
    expect(ends.has("reviewEvent")).toBe(true);
    expect(ends.has("decision")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching submitting passes through SIGN_IN, OPEN, DECIDE and CONFIRM", () => {
    const paths = getShortestPaths(moderateMachine, {
      input: inputFor(okModerate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("SIGN_IN");
    expect(events).toContain("OPEN");
    expect(events).toContain("DECIDE");
    expect(events).toContain("CONFIRM");
  });
});

describe("moderateMachine \u{2014} telemetry events (happy path)", () => {
  it("sign-in -> open -> decide -> confirm -> moderated fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(moderateMachine, {
      input: inputFor(okModerate, track),
    }).start();

    expect(track.mock.calls.map((c) => c[0])).toContain(MODERATE_EVENTS.gateViewed);

    actor.send({ type: "SIGN_IN" });
    expect(actor.getSnapshot().matches("queue")).toBe(true);

    actor.send({ type: "OPEN", eventId: "evt-9" });
    expect(actor.getSnapshot().matches("reviewEvent")).toBe(true);
    expect(actor.getSnapshot().context.eventId).toBe("evt-9");

    actor.send({ type: "DECIDE", action: "approve" });
    expect(actor.getSnapshot().matches("decision")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("moderated"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(MODERATE_EVENTS.gateViewed);
    expect(events).toContain(MODERATE_EVENTS.authenticated);
    expect(events).toContain(MODERATE_EVENTS.queueViewed);
    expect(events).toContain(MODERATE_EVENTS.eventOpened);
    expect(events).toContain(MODERATE_EVENTS.decisionMade);
    expect(events).toContain(MODERATE_EVENTS.confirmed);
    expect(events).toContain(MODERATE_EVENTS.moderated);

    expect(events.indexOf(MODERATE_EVENTS.confirmed)).toBeLessThan(
      events.indexOf(MODERATE_EVENTS.moderated),
    );

    const decideCall = track.mock.calls.find((c) => c[0] === MODERATE_EVENTS.decisionMade);
    expect(decideCall?.[1]).toMatchObject({ event_id: "evt-9", action: "approve" });
    expect(decideCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "lp_whatson_admin_moderation",
      variant: "moderation_wizard",
    });
    expect(actor.getSnapshot().context.result).toMatchObject({ id: "evt-9" });
  });

  it("CONTINUE returns to the queue and clears the selection", async () => {
    const track = vi.fn();
    const actor = createActor(moderateMachine, {
      input: inputFor(okModerate, track),
    }).start();

    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "OPEN", eventId: "evt-2" });
    actor.send({ type: "DECIDE", action: "feature" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("moderated"));

    actor.send({ type: "CONTINUE" });
    expect(actor.getSnapshot().matches("queue")).toBe(true);
    expect(actor.getSnapshot().context.eventId).toBeUndefined();
    expect(actor.getSnapshot().context.action).toBeUndefined();
  });

  it("reject path carries reasons and never auto-approves", () => {
    const track = vi.fn();
    const moderate = vi.fn(okModerate);
    const actor = createActor(moderateMachine, {
      input: inputFor(moderate, track),
    }).start();

    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "OPEN", eventId: "evt-5" });
    actor.send({
      type: "DECIDE",
      action: "reject",
      rejectReasons: ["invalid_image", "invalid_location"],
      rejectNote: "blurry poster",
    });
    expect(actor.getSnapshot().matches("decision")).toBe(true);
    expect(actor.getSnapshot().context.action).toBe("reject");
    expect(actor.getSnapshot().context.rejectReasons).toEqual([
      "invalid_image",
      "invalid_location",
    ]);
    expect(moderate).not.toHaveBeenCalled();
  });
});

describe("moderateMachine \u{2014} moderate failure + retry", () => {
  it("moderate error -> back to decision -> CONFIRM recovers to moderated", async () => {
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

    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "OPEN", eventId: "evt-6" });
    actor.send({ type: "DECIDE", action: "archive" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("decision") && s.context.error !== undefined);
    expect(actor.getSnapshot().context.error).toBe("catalyst unreachable");

    const failEvents = track.mock.calls.map((c) => c[0]);
    expect(failEvents).toContain(MODERATE_EVENTS.failed);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("moderated"));
    expect(track.mock.calls.map((c) => c[0])).toContain(MODERATE_EVENTS.moderated);
  });

  it("failModerate always rejects (sanity)", async () => {
    await expect(failModerate({ eventId: "x", action: "approve" })).rejects.toThrow();
  });
});

describe("simulateModerateAction", () => {
  it("resolves the {id,local} patch envelope keyed by action (no network)", async () => {
    const approved = await simulateModerateAction({ eventId: "e1", action: "approve" });
    const rejected = await simulateModerateAction({
      eventId: "e2",
      action: "reject",
      rejectReasons: ["invalid_image"],
    });
    expect(approved.simulated).toBe(true);
    expect(approved.id).toBe("e1");
    expect(approved.local).toMatchObject({ approved: true, rejected: false });
    expect(rejected.local).toMatchObject({ approved: false, rejected: true });
    expect(String(rejected.local.rejection_reason)).toContain("invalid_image");
  });
});

describe("whatson-admin-moderate \u{2014} the default actor fails closed", () => {
  it("failClosedModerateAction rejects instead of reporting a fake success", async () => {
    await expect(
      failClosedModerateAction({ eventId: "evt-1", action: "approve" }),
    ).rejects.toThrow(/not available on this node/i);
  });

  it("a machine with no injected `moderate` lands in an error state, never approved", async () => {
    const actor = createActor(moderateMachine, {
      input: {
        trackCtx: {
          sid: "sid-1",
          story: "landings-whatson-admin-moderate",
          variant: "v",
          experimentKey: "k",
        },
      },
    }).start();

    // Whatever the wizard's happy path is, the default write must not succeed.
    await expect(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (actor.getSnapshot().context.moderate as ModerateFn)({
        eventId: "evt-1",
        action: "approve",
      }),
    ).rejects.toThrow();
    actor.stop();
  });
});
