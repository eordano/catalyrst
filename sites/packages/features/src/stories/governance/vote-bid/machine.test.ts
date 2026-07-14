import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  bidVoteMachine,
  BID_VOTE_EVENTS,
  BID_CHOICES,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveBidVoteSnapshot,
  slugToState,
  stateToSlug,
  simulateCast,
  type CastFn,
  type CastResult,
  type TrackFn,
} from "./machine";

const RESULT: CastResult = { receipt: "sim:bid-1:yes" };

const okCast: CastFn = async () => RESULT;
const failCast: CastFn = async () => {
  throw new Error("snapshot unreachable");
};

function inputFor(cast: CastFn, track: TrackFn, maxErrors = 2) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-vote-bid",
      variant: "gated",
      experimentKey: "gv_bid_vote_flow",
    },
    bidId: "bid-1",
    fieldSize: 6,
    maxErrors,
    cast,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "review",
  "choosing",
  "casting",
  "error",
  "snapshot",
  "completed",
]);

const TRAVERSAL_EVENTS = [
  { type: "ACKNOWLEDGE" as const },
  { type: "SELECT_CHOICE" as const, choice: "Yes" as const },
  { type: "CAST" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
  { type: "REDIRECT" as const },
];

describe("bidVoteMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(bidVoteMachine.states));
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

  it("the five audit-spec steps are all addressable slugs", () => {
    for (const step of ["review", "choosing", "casting", "error", "snapshot"]) {
      expect(SLUG_TO_STATE[step as keyof typeof SLUG_TO_STATE]).toBeDefined();
    }
  });

  it("unknown/missing ?step falls back to the first step (review)", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.review);
    expect(slugToState(null)).toBe("review");
    expect(slugToState(undefined)).toBe("review");
    expect(slugToState("")).toBe("review");
    expect(slugToState("nope")).toBe("review");
    expect(slugToState("choosing")).toBe("choosing");
    expect(slugToState("casting")).toBe("casting");
    expect(slugToState("snapshot")).toBe("snapshot");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("bidVoteMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveBidVoteSnapshot({
      step: "review",
      trackCtx: inputFor(okCast, () => {}).trackCtx,
      bidId: "bid-1",
      fieldSize: 6,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating casting does NOT fire telemetry and does NOT auto-cast", async () => {
    const track = vi.fn();
    const cast = vi.fn(okCast);
    const snapshot = resolveBidVoteSnapshot({
      step: "casting",
      trackCtx: inputFor(cast, track).trackCtx,
      bidId: "bid-1",
      fieldSize: 6,
      cast,
      track,
    });
    const actor = createActor(bidVoteMachine, {
      input: inputFor(cast, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("casting")).toBe(true);
    expect(actor.getSnapshot().context.choice).toBe("Yes");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(cast).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("casting")).toBe(true);
  });

  it("hydrating snapshot does NOT re-fire the redirect event", () => {
    const track = vi.fn();
    const snapshot = resolveBidVoteSnapshot({
      step: "snapshot",
      trackCtx: inputFor(okCast, track).trackCtx,
      bidId: "bid-1",
      fieldSize: 6,
      track,
    });
    const actor = createActor(bidVoteMachine, {
      input: inputFor(okCast, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("snapshot")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydrating choosing still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveBidVoteSnapshot({
      step: "choosing",
      trackCtx: inputFor(okCast, track).trackCtx,
      bidId: "bid-1",
      fieldSize: 6,
      track,
    });
    const actor = createActor(bidVoteMachine, {
      input: inputFor(okCast, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("choosing")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SELECT_CHOICE", choice: "No" });
    expect(actor.getSnapshot().context.choice).toBe("No");
    expect(track.mock.calls.map((c) => c[0])).toContain(BID_VOTE_EVENTS.choiceSelected);
  });
});

describe("bidVoteMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(bidVoteMachine, {
      input: inputFor(okCast, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("review")).toBe(true);
    expect(ends.has("choosing")).toBe(true);
    expect(ends.has("casting")).toBe(true);
  });

  it("reaching casting passes through ACKNOWLEDGE, SELECT_CHOICE and CAST", () => {
    const paths = getShortestPaths(bidVoteMachine, {
      input: inputFor(okCast, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const casting = paths.find((p) => (p.state.value as string) === "casting");
    expect(casting).toBeDefined();
    const events = casting!.steps.map((s) => s.event.type);
    expect(events).toContain("ACKNOWLEDGE");
    expect(events).toContain("SELECT_CHOICE");
    expect(events).toContain("CAST");
  });
});

describe("bidVoteMachine \u{2014} the reckon gate", () => {
  it("CAST is blocked until a choice is picked (hasChoice guard)", () => {
    const track = vi.fn();
    const actor = createActor(bidVoteMachine, {
      input: inputFor(okCast, track),
    }).start();

    actor.send({ type: "ACKNOWLEDGE" });
    expect(actor.getSnapshot().matches("choosing")).toBe(true);

    actor.send({ type: "CAST" });
    expect(actor.getSnapshot().matches("choosing")).toBe(true);

    actor.send({ type: "SELECT_CHOICE", choice: "Yes" });
    actor.send({ type: "CAST" });
    expect(actor.getSnapshot().matches("casting")).toBe(true);
  });

  it("you cannot reach choosing without acknowledging the field", () => {
    const actor = createActor(bidVoteMachine, {
      input: inputFor(okCast, () => {}),
    }).start();
    expect(actor.getSnapshot().matches("review")).toBe(true);
    actor.send({ type: "SELECT_CHOICE", choice: "Yes" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });
});

describe("bidVoteMachine \u{2014} telemetry events (happy path)", () => {
  it("review -> choosing -> cast -> completed fires the full funnel in order", async () => {
    const track = vi.fn();
    const actor = createActor(bidVoteMachine, {
      input: inputFor(okCast, track),
    }).start();

    expect(track.mock.calls.map((c) => c[0])).toContain(BID_VOTE_EVENTS.started);

    actor.send({ type: "ACKNOWLEDGE" });
    actor.send({ type: "SELECT_CHOICE", choice: "Yes" });
    actor.send({ type: "CAST" });
    await waitFor(actor, (s) => s.matches("completed"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(BID_VOTE_EVENTS.started);
    expect(events).toContain(BID_VOTE_EVENTS.fieldReviewed);
    expect(events).toContain(BID_VOTE_EVENTS.choiceSelected);
    expect(events).toContain(BID_VOTE_EVENTS.castReached);
    expect(events).toContain(BID_VOTE_EVENTS.completed);

    expect(events.indexOf(BID_VOTE_EVENTS.started)).toBeLessThan(
      events.indexOf(BID_VOTE_EVENTS.fieldReviewed),
    );
    expect(events.indexOf(BID_VOTE_EVENTS.castReached)).toBeLessThan(
      events.indexOf(BID_VOTE_EVENTS.completed),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === BID_VOTE_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_bid_vote_flow",
      variant: "gated",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("bidVoteMachine \u{2014} cast failure + retry + snapshot escalation", () => {
  it("first failure goes to error; RETRY recovers to completed", async () => {
    const track = vi.fn();
    let calls = 0;
    const cast: CastFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("snapshot unreachable");
      return okCast(args);
    };
    const actor = createActor(bidVoteMachine, {
      input: inputFor(cast, track, 2),
    }).start();

    actor.send({ type: "ACKNOWLEDGE" });
    actor.send({ type: "SELECT_CHOICE", choice: "No" });
    actor.send({ type: "CAST" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("snapshot unreachable");
    expect(actor.getSnapshot().context.attempts).toBe(1);

    const failedCall = track.mock.calls.find((c) => c[0] === BID_VOTE_EVENTS.castFailed);
    expect(failedCall?.[1]).toMatchObject({ attempt: 1 });

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("completed"));
    expect(track.mock.calls.map((c) => c[0])).toContain(BID_VOTE_EVENTS.completed);
  });

  it("repeated failures escalate to the Snapshot redirect (atMaxErrors)", async () => {
    const track = vi.fn();
    const actor = createActor(bidVoteMachine, {
      input: inputFor(failCast, track, 2),
    }).start();

    actor.send({ type: "ACKNOWLEDGE" });
    actor.send({ type: "SELECT_CHOICE", choice: "Yes" });

    actor.send({ type: "CAST" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.attempts).toBe(1);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("snapshot"));
    expect(actor.getSnapshot().context.attempts).toBe(2);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(BID_VOTE_EVENTS.snapshotRedirect);
    expect(events).not.toContain(BID_VOTE_EVENTS.completed);
  });

  it("the error screen also offers a direct REDIRECT to snapshot", async () => {
    const track = vi.fn();
    let calls = 0;
    const cast: CastFn = async () => {
      calls += 1;
      throw new Error("fail");
    };
    const actor = createActor(bidVoteMachine, {
      input: inputFor(cast, track, 5),
    }).start();

    actor.send({ type: "ACKNOWLEDGE" });
    actor.send({ type: "SELECT_CHOICE", choice: "Yes" });
    actor.send({ type: "CAST" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "REDIRECT" });
    expect(actor.getSnapshot().matches("snapshot")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(BID_VOTE_EVENTS.snapshotRedirect);
    expect(calls).toBe(1);
  });
});

describe("simulateCast", () => {
  it("resolves a deterministic receipt keyed by bid + choice (no network)", async () => {
    const r = await simulateCast({ bidId: "abc", choice: "Yes", attempt: 1 });
    expect(r.receipt).toBe("sim:abc:yes");
  });

  it("BID_CHOICES are the Snapshot single-choice options", () => {
    expect(BID_CHOICES).toEqual(["Yes", "No", "Abstain"]);
  });
});
