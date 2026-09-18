import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  bidVoteMachine,
  BID_VOTE_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveBidVoteSnapshot,
  slugToState,
  stateToSlug,
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

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("bidVoteMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to review", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(bidVoteMachine.states)));
    expect(mapped).toEqual(EXPECTED_STATES);
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.review);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("review");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("bidVoteMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots review without a snapshot, hydrates casting/snapshot silently, and only real transitions track", async () => {
    const track = vi.fn();
    const cast = vi.fn(okCast);
    const trackCtx = inputFor(cast, track).trackCtx;
    const base = { trackCtx, bidId: "bid-1", fieldSize: 6 };
    expect(resolveBidVoteSnapshot({ step: "review", ...base })).toBeUndefined();

    const casting = createActor(bidVoteMachine, {
      input: inputFor(cast, track),
      snapshot: resolveBidVoteSnapshot({ step: "casting", ...base, cast, track }),
    }).start();
    expect(casting.getSnapshot().matches("casting")).toBe(true);
    expect(casting.getSnapshot().context.choice).toBe("Yes");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(cast).not.toHaveBeenCalled();
    expect(casting.getSnapshot().matches("casting")).toBe(true);

    const redirected = createActor(bidVoteMachine, {
      input: inputFor(okCast, track),
      snapshot: resolveBidVoteSnapshot({ step: "snapshot", ...base, track }),
    }).start();
    expect(redirected.getSnapshot().matches("snapshot")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    const choosing = createActor(bidVoteMachine, {
      input: inputFor(okCast, track),
      snapshot: resolveBidVoteSnapshot({ step: "choosing", ...base, track }),
    }).start();
    expect(choosing.getSnapshot().matches("choosing")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    choosing.send({ type: "SELECT_CHOICE", choice: "No" });
    expect(choosing.getSnapshot().context.choice).toBe("No");
    expect(names(track)).toContain(BID_VOTE_EVENTS.choiceSelected);
  });
});

describe("bidVoteMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and casting needs ACKNOWLEDGE, SELECT_CHOICE, CAST", () => {
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
    for (const s of ["review", "choosing", "casting"]) expect(ends.has(s)).toBe(true);
    const casting = paths.find((p) => (p.state.value as string) === "casting");
    const events = casting!.steps.map((s) => s.event.type);
    for (const e of ["ACKNOWLEDGE", "SELECT_CHOICE", "CAST"]) expect(events).toContain(e);
  });
});

describe("bidVoteMachine \u{2014} the reckon gate", () => {
  it("choosing needs an acknowledged field, and CAST needs a picked choice", () => {
    const actor = createActor(bidVoteMachine, {
      input: inputFor(okCast, vi.fn()),
    }).start();
    expect(actor.getSnapshot().matches("review")).toBe(true);
    actor.send({ type: "SELECT_CHOICE", choice: "Yes" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "ACKNOWLEDGE" });
    expect(actor.getSnapshot().matches("choosing")).toBe(true);
    actor.send({ type: "CAST" });
    expect(actor.getSnapshot().matches("choosing")).toBe(true);

    actor.send({ type: "SELECT_CHOICE", choice: "Yes" });
    actor.send({ type: "CAST" });
    expect(actor.getSnapshot().matches("casting")).toBe(true);
  });
});

describe("bidVoteMachine \u{2014} telemetry events (happy path)", () => {
  it("review -> choosing -> cast -> completed fires the full funnel in order", async () => {
    const track = vi.fn();
    const actor = createActor(bidVoteMachine, {
      input: inputFor(okCast, track),
    }).start();
    expect(names(track)).toContain(BID_VOTE_EVENTS.started);

    actor.send({ type: "ACKNOWLEDGE" });
    actor.send({ type: "SELECT_CHOICE", choice: "Yes" });
    actor.send({ type: "CAST" });
    await waitFor(actor, (s) => s.matches("completed"));

    const events = names(track);
    for (const e of [
      BID_VOTE_EVENTS.started,
      BID_VOTE_EVENTS.fieldReviewed,
      BID_VOTE_EVENTS.choiceSelected,
      BID_VOTE_EVENTS.castReached,
      BID_VOTE_EVENTS.completed,
    ]) {
      expect(events).toContain(e);
    }
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
  it("first failure goes to error with the attempt count; RETRY recovers to completed", async () => {
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
    expect(names(track)).toContain(BID_VOTE_EVENTS.completed);
  });

  it("repeated failures escalate to the Snapshot redirect, and the error screen offers a direct REDIRECT", async () => {
    const escalated = vi.fn();
    const auto = createActor(bidVoteMachine, {
      input: inputFor(failCast, escalated, 2),
    }).start();
    auto.send({ type: "ACKNOWLEDGE" });
    auto.send({ type: "SELECT_CHOICE", choice: "Yes" });
    auto.send({ type: "CAST" });
    await waitFor(auto, (s) => s.matches("error"));
    expect(auto.getSnapshot().context.attempts).toBe(1);
    auto.send({ type: "RETRY" });
    await waitFor(auto, (s) => s.matches("snapshot"));
    expect(auto.getSnapshot().context.attempts).toBe(2);
    expect(names(escalated)).toContain(BID_VOTE_EVENTS.snapshotRedirect);
    expect(names(escalated)).not.toContain(BID_VOTE_EVENTS.completed);

    const track = vi.fn();
    let calls = 0;
    const cast: CastFn = async () => {
      calls += 1;
      throw new Error("fail");
    };
    const direct = createActor(bidVoteMachine, {
      input: inputFor(cast, track, 5),
    }).start();
    direct.send({ type: "ACKNOWLEDGE" });
    direct.send({ type: "SELECT_CHOICE", choice: "Yes" });
    direct.send({ type: "CAST" });
    await waitFor(direct, (s) => s.matches("error"));
    direct.send({ type: "REDIRECT" });
    expect(direct.getSnapshot().matches("snapshot")).toBe(true);
    expect(names(track)).toContain(BID_VOTE_EVENTS.snapshotRedirect);
    expect(calls).toBe(1);
  });
});
