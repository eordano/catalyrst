import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  bidMachine,
  BID_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveBidSnapshot,
  slugToState,
  stateToSlug,
  failClosedSubmit,
  type SubmitFn,
  type SubmitResult,
  type TrackFn,
} from "./machine";

const TENDER_ID = "b78f6e4e-baaa-4256-97c7-e78e90cb55ab";
const RESULT: SubmitResult = { proposalId: "bid-b78f6e4e-abc", published: false };

const okSubmit: SubmitFn = async () => RESULT;

function inputFor(submitBid: SubmitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-bid",
      variant: "wizard",
      experimentKey: "gv_bid_wizard",
    },
    tenderId: TENDER_ID,
    submitBid,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "parents",
  "funding",
  "general",
  "review",
  "submitting",
  "submitError",
  "success",
]);

const TRAVERSAL_EVENTS = [
  { type: "CONTINUE" as const },
  { type: "SET_FUNDING" as const, budget: 90000, duration: 4 },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

describe("bidMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, routes every spec ?step, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(bidMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }

    for (const step of ["parents", "funding", "general", "review", "submitting", "success"]) {
      expect(EXPECTED_STATES.has(slugToState(step))).toBe(true);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.parents);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("parents");
    }
    expect(slugToState("funding")).toBe("funding");
    expect(slugToState("submit-error")).toBe("submitError");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("bidMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot; submitting hydrates without telemetry or auto-submit; a real transition after hydration fires", async () => {
    const trackCtx = inputFor(okSubmit, () => {}).trackCtx;
    expect(resolveBidSnapshot({ step: "parents", trackCtx, tenderId: TENDER_ID })).toBeUndefined();

    const track = vi.fn();
    const submitBid = vi.fn(okSubmit);
    const submitting = createActor(bidMachine, {
      input: inputFor(submitBid, track),
      snapshot: resolveBidSnapshot({ step: "submitting", trackCtx, tenderId: TENDER_ID, submitBid, track }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.draft.tenderId).toBe(TENDER_ID);
    expect(submitting.getSnapshot().context.draft.budget).toBe(90000);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submitBid).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const review = createActor(bidMachine, {
      input: inputFor(okSubmit, track),
      snapshot: resolveBidSnapshot({ step: "review", trackCtx, tenderId: TENDER_ID, track }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    review.send({ type: "SUBMIT" });
    expect(review.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(BID_EVENTS.submitAttempted);
  });
});

describe("bidMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and submitting passes through the full step sequence", () => {
    const paths = getShortestPaths(bidMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    for (const s of ["funding", "general", "review", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }

    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    expect(submitting!.steps.map((s) => s.event.type)).toEqual(
      expect.arrayContaining(["CONTINUE", "SET_FUNDING", "NEXT", "SUBMIT"]),
    );
  });
});

describe("bidMachine \u{2014} telemetry events (happy path)", () => {
  it("full flow fires the complete funnel in order with one step-advanced (general->review); BACK steps never re-fire forward telemetry", async () => {
    const track = vi.fn();
    const actor = createActor(bidMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "CONTINUE" });
    expect(actor.getSnapshot().matches("funding")).toBe(true);

    actor.send({ type: "SET_FUNDING", budget: 90000, duration: 4 });
    expect(actor.getSnapshot().matches("general")).toBe(true);

    actor.send({ type: "NEXT" });
    const advanced = track.mock.calls.filter((c) => c[0] === BID_EVENTS.stepAdvanced);
    expect(advanced.length).toBe(1);
    expect((advanced[0][1] as { to: string }).to).toBe("review");

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        BID_EVENTS.started,
        BID_EVENTS.fundingSet,
        BID_EVENTS.stepAdvanced,
        BID_EVENTS.submitAttempted,
        BID_EVENTS.submitted,
      ]),
    );
    expect(events.indexOf(BID_EVENTS.submitAttempted)).toBeLessThan(
      events.indexOf(BID_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === BID_EVENTS.started);
    expect(startedCall?.[1]).toMatchObject({ tender_id: TENDER_ID });
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_bid_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
    const submittedCall = track.mock.calls.find((c) => c[0] === BID_EVENTS.submitted);
    expect(submittedCall?.[1]).toMatchObject({ published: false });

    const backTrack = vi.fn();
    const back = createActor(bidMachine, {
      input: inputFor(okSubmit, backTrack),
    }).start();
    back.send({ type: "CONTINUE" });
    back.send({ type: "SET_FUNDING", budget: 1000, duration: 1 });
    expect(back.getSnapshot().matches("general")).toBe(true);
    back.send({ type: "BACK" });
    expect(back.getSnapshot().matches("funding")).toBe(true);
    back.send({ type: "BACK" });
    expect(back.getSnapshot().matches("parents")).toBe(true);
    expect(backTrack.mock.calls.filter((c) => c[0] === BID_EVENTS.started).length).toBe(1);
  });
});

describe("bidMachine \u{2014} submit failure + retry", () => {
  it("submit error -> BACK returns to review without submitting; a second failure -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submitBid: SubmitFn = async (args) => {
      calls += 1;
      if (calls <= 2) throw new Error("governance api unreachable");
      return okSubmit(args);
    };

    const actor = createActor(bidMachine, {
      input: inputFor(submitBid, track),
    }).start();

    actor.send({ type: "CONTINUE" });
    actor.send({ type: "SET_FUNDING", budget: 90000, duration: 4 });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));
    expect(actor.getSnapshot().context.error).toBe("governance api unreachable");

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(BID_EVENTS.submitted);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(BID_EVENTS.submitted);
    expect(calls).toBe(3);
  });
});

describe("failClosedSubmit", () => {
  it("fails closed instead of fabricating a bid id", async () => {
    await expect(
      failClosedSubmit({ tenderId: TENDER_ID, budget: 1000, duration: 2 }),
    ).rejects.toThrow("bid submission unavailable: DAO governance signer not configured");
  });
});
