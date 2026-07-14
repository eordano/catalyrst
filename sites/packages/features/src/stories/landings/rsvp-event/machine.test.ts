import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  rsvpMachine,
  RSVP_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveRsvpSnapshot,
  slugToState,
  stateToSlug,
  simulateCommit,
  type CommitFn,
  type RsvpResult,
  type TrackFn,
} from "./machine";

const EVENT_ID = "b8aa88d2-03ff-4453-825a-3f2e7ac00ecc";

const okCommit: CommitFn = async ({ direction, count }) => ({
  count: direction === "going" ? count + 1 : Math.max(0, count - 1),
});
const failCommit: CommitFn = async () => {
  throw new Error("auth rejected");
};

function inputFor(commit: CommitFn, track: TrackFn, count = 7) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "landings-rsvp-event",
      variant: "confirm",
      experimentKey: "lp_rsvp_confirm",
    },
    eventId: EVENT_ID,
    count,
    commit,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "idle",
  "signinGate",
  "confirming",
  "submitting",
  "going",
  "cancelling",
  "notGoing",
  "error",
]);

describe("rsvpMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(rsvpMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);
  });

  it("slugs are unique, match the audit-spec step names, and round-trip", () => {
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    expect(new Set(slugs)).toEqual(
      new Set([
        "idle",
        "signin-gate",
        "confirming",
        "submitting",
        "going",
        "cancelling",
        "not-going",
        "error",
      ]),
    );
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.idle);
    expect(slugToState(null)).toBe("idle");
    expect(slugToState(undefined)).toBe("idle");
    expect(slugToState("")).toBe("idle");
    expect(slugToState("nope")).toBe("idle");
    expect(slugToState("signin-gate")).toBe("signinGate");
    expect(slugToState("confirming")).toBe("confirming");
    expect(slugToState("not-going")).toBe("notGoing");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("rsvpMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveRsvpSnapshot({
      step: "idle",
      trackCtx: inputFor(okCommit, () => {}).trackCtx,
      eventId: EVENT_ID,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-commit", async () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const snapshot = resolveRsvpSnapshot({
      step: "submitting",
      trackCtx: inputFor(commit, track).trackCtx,
      eventId: EVENT_ID,
      count: 7,
      commit,
      track,
    });
    const actor = createActor(rsvpMachine, {
      input: inputFor(commit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("hydrating confirming does NOT fire the entry confirmed event", () => {
    const track = vi.fn();
    const snapshot = resolveRsvpSnapshot({
      step: "confirming",
      trackCtx: inputFor(okCommit, track).trackCtx,
      eventId: EVENT_ID,
      track,
    });
    const actor = createActor(rsvpMachine, {
      input: inputFor(okCommit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("idle")).toBe(true);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "TAP_GOING" as const },
  { type: "SIGN_IN" as const },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "CANCEL" as const },
  { type: "CANCEL_RSVP" as const },
  { type: "RETRY" as const },
  { type: "DISMISS" as const },
];

describe("rsvpMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(rsvpMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("signinGate")).toBe(true);
    expect(ends.has("confirming")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching submitting passes the auth gate (TAP_GOING -> SIGN_IN -> CONFIRM)", () => {
    const paths = getShortestPaths(rsvpMachine, {
      input: inputFor(okCommit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("TAP_GOING");
    expect(events).toContain("SIGN_IN");
    expect(events).toContain("CONFIRM");
  });
});

describe("rsvpMachine \u{2014} RSVP going (happy path)", () => {
  it("idle -> signin -> confirm -> submit -> going fires the full funnel and increments the count", async () => {
    const track = vi.fn();
    const actor = createActor(rsvpMachine, {
      input: inputFor(okCommit, track, 7),
    }).start();

    actor.send({ type: "TAP_GOING" });
    expect(actor.getSnapshot().matches("signinGate")).toBe(true);

    actor.send({ type: "SIGN_IN" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("going"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(RSVP_EVENTS.started);
    expect(events).toContain(RSVP_EVENTS.signin);
    expect(events).toContain(RSVP_EVENTS.confirmed);
    expect(events).toContain(RSVP_EVENTS.submitting);
    expect(events).toContain(RSVP_EVENTS.going);

    expect(events.indexOf(RSVP_EVENTS.confirmed)).toBeLessThan(
      events.indexOf(RSVP_EVENTS.going),
    );

    expect(actor.getSnapshot().context.count).toBe(8);

    const goingCall = track.mock.calls.find((c) => c[0] === RSVP_EVENTS.going);
    expect(goingCall?.[1]).toMatchObject({ event_id: EVENT_ID, stub: true });
    expect(goingCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "lp_rsvp_confirm",
      variant: "confirm",
    });
  });

  it("CANCEL from the sign-in gate returns to idle without committing", () => {
    const track = vi.fn();
    const commit = vi.fn(okCommit);
    const actor = createActor(rsvpMachine, {
      input: inputFor(commit, track),
    }).start();

    actor.send({ type: "TAP_GOING" });
    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("idle")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(RSVP_EVENTS.started);
    expect(events).not.toContain(RSVP_EVENTS.confirmed);
    expect(commit).not.toHaveBeenCalled();
  });
});

describe("rsvpMachine \u{2014} cancel RSVP", () => {
  it("going -> cancel -> notGoing fires cancelling + cancelled and decrements the count", async () => {
    const track = vi.fn();
    const actor = createActor(rsvpMachine, {
      input: inputFor(okCommit, track, 7),
    }).start();

    actor.send({ type: "TAP_GOING" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("going"));
    expect(actor.getSnapshot().context.count).toBe(8);

    actor.send({ type: "CANCEL_RSVP" });
    await waitFor(actor, (s) => s.matches("notGoing"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(RSVP_EVENTS.cancelling);
    expect(events).toContain(RSVP_EVENTS.cancelled);
    expect(actor.getSnapshot().context.count).toBe(7);

    actor.send({ type: "TAP_GOING" });
    expect(actor.getSnapshot().matches("signinGate")).toBe(true);
  });
});

describe("rsvpMachine \u{2014} commit failure + retry", () => {
  it("submit error fires lp_rsvp_error then RETRY recovers to going", async () => {
    const track = vi.fn();
    let calls = 0;
    const commit: CommitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("auth rejected");
      return okCommit(args);
    };

    const actor = createActor(rsvpMachine, {
      input: inputFor(commit, track, 7),
    }).start();

    actor.send({ type: "TAP_GOING" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("auth rejected");

    const errEvents = track.mock.calls.map((c) => c[0]);
    expect(errEvents).toContain(RSVP_EVENTS.error);
    const errCall = track.mock.calls.find((c) => c[0] === RSVP_EVENTS.error);
    expect(errCall?.[1]).toMatchObject({ reason: "auth rejected" });

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("going"));
    expect(track.mock.calls.map((c) => c[0])).toContain(RSVP_EVENTS.going);
  });

  it("DISMISS from error returns to idle", async () => {
    const actor = createActor(rsvpMachine, {
      input: inputFor(failCommit, () => {}, 7),
    }).start();

    actor.send({ type: "TAP_GOING" });
    actor.send({ type: "SIGN_IN" });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "DISMISS" });
    expect(actor.getSnapshot().matches("idle")).toBe(true);
  });
});

describe("simulateCommit", () => {
  it("increments on going, decrements (floored at 0) on cancel \u{2014} no network", async () => {
    const up = await simulateCommit({ eventId: EVENT_ID, direction: "going", count: 7 });
    const down = await simulateCommit({ eventId: EVENT_ID, direction: "cancel", count: 7 });
    const floor = await simulateCommit({ eventId: EVENT_ID, direction: "cancel", count: 0 });
    expect(up.count).toBe(8);
    expect(down.count).toBe(6);
    expect(floor.count).toBe(0);
  });
});
