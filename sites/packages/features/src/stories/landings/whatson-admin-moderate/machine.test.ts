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

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("moderateMachine \u{2014} URL ?step slug map", () => {
  it("uses the audit-spec step ids, unique and round-tripping, falling back to auth-gate", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(moderateMachine.states)));
    expect(mapped).toEqual(EXPECTED_STATES);
    expect(Object.values(STATE_TO_SLUG)).toEqual([
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
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.authGate);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("authGate");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("moderateMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots authGate without a snapshot, hydrates submitting/queue silently, and a reject decision carries reasons without auto-moderating", async () => {
    const track = vi.fn();
    const moderate = vi.fn(okModerate);
    const trackCtx = inputFor(moderate, track).trackCtx;
    expect(resolveModerateSnapshot({ step: "authGate", trackCtx })).toBeUndefined();

    const submitting = createActor(moderateMachine, {
      input: inputFor(moderate, track),
      snapshot: resolveModerateSnapshot({
        step: "submitting",
        trackCtx,
        moderate,
        track,
        eventId: "evt-7",
        action: "approve",
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.eventId).toBe("evt-7");
    expect(submitting.getSnapshot().context.action).toBe("approve");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(moderate).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const queue = createActor(moderateMachine, {
      input: inputFor(moderate, track),
      snapshot: resolveModerateSnapshot({ step: "queue", trackCtx, track }),
    }).start();
    expect(queue.getSnapshot().matches("queue")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    const review = createActor(moderateMachine, {
      input: inputFor(moderate, track),
      snapshot: resolveModerateSnapshot({ step: "reviewEvent", trackCtx, track, eventId: "evt-3" }),
    }).start();
    expect(review.getSnapshot().matches("reviewEvent")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    review.send({
      type: "DECIDE",
      action: "reject",
      rejectReasons: ["invalid_image", "invalid_location"],
      rejectNote: "blurry poster",
    });
    expect(review.getSnapshot().matches("decision")).toBe(true);
    expect(review.getSnapshot().context.action).toBe("reject");
    expect(review.getSnapshot().context.rejectReasons).toEqual([
      "invalid_image",
      "invalid_location",
    ]);
    expect(names(track)).toContain(MODERATE_EVENTS.decisionMade);
    expect(moderate).not.toHaveBeenCalled();
  });
});

describe("moderateMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and submitting needs SIGN_IN, OPEN, DECIDE, CONFIRM", () => {
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
    for (const s of ["queue", "reviewEvent", "decision", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    const events = submitting!.steps.map((s) => s.event.type);
    for (const e of ["SIGN_IN", "OPEN", "DECIDE", "CONFIRM"]) expect(events).toContain(e);
  });
});

describe("moderateMachine \u{2014} telemetry events (happy path)", () => {
  it("sign-in -> open -> decide -> confirm -> moderated fires the full funnel, then CONTINUE clears the selection", async () => {
    const track = vi.fn();
    const actor = createActor(moderateMachine, {
      input: inputFor(okModerate, track),
    }).start();
    expect(names(track)).toContain(MODERATE_EVENTS.gateViewed);

    actor.send({ type: "SIGN_IN" });
    expect(actor.getSnapshot().matches("queue")).toBe(true);
    actor.send({ type: "OPEN", eventId: "evt-9" });
    expect(actor.getSnapshot().matches("reviewEvent")).toBe(true);
    expect(actor.getSnapshot().context.eventId).toBe("evt-9");
    actor.send({ type: "DECIDE", action: "approve" });
    expect(actor.getSnapshot().matches("decision")).toBe(true);
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("moderated"));

    const events = names(track);
    for (const e of [
      MODERATE_EVENTS.gateViewed,
      MODERATE_EVENTS.authenticated,
      MODERATE_EVENTS.queueViewed,
      MODERATE_EVENTS.eventOpened,
      MODERATE_EVENTS.decisionMade,
      MODERATE_EVENTS.confirmed,
      MODERATE_EVENTS.moderated,
    ]) {
      expect(events).toContain(e);
    }
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

    actor.send({ type: "CONTINUE" });
    expect(actor.getSnapshot().matches("queue")).toBe(true);
    expect(actor.getSnapshot().context.eventId).toBeUndefined();
    expect(actor.getSnapshot().context.action).toBeUndefined();
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
    expect(names(track)).toContain(MODERATE_EVENTS.failed);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("moderated"));
    expect(names(track)).toContain(MODERATE_EVENTS.moderated);
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
  it("failClosedModerateAction and a machine with no injected moderate both reject, never approve", async () => {
    await expect(
      failClosedModerateAction({ eventId: "evt-1", action: "approve" }),
    ).rejects.toThrow(/not available on this node/i);

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
    await expect(
      (actor.getSnapshot().context.moderate as ModerateFn)({
        eventId: "evt-1",
        action: "approve",
      }),
    ).rejects.toThrow();
    actor.stop();
  });
});
