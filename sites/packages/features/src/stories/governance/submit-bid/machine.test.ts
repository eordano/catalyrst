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
const failSubmit: SubmitFn = async () => {
  throw new Error("governance api unreachable");
};

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
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(bidMachine.states));
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

  it("spec ?step values are all routable", () => {
    for (const step of [
      "parents",
      "funding",
      "general",
      "review",
      "submitting",
      "success",
    ]) {
      expect(EXPECTED_STATES.has(slugToState(step))).toBe(true);
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.parents);
    expect(slugToState(null)).toBe("parents");
    expect(slugToState(undefined)).toBe("parents");
    expect(slugToState("")).toBe("parents");
    expect(slugToState("nope")).toBe("parents");
    expect(slugToState("funding")).toBe("funding");
    expect(slugToState("submit-error")).toBe("submitError");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("bidMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveBidSnapshot({
      step: "parents",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
      tenderId: TENDER_ID,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submitBid = vi.fn(okSubmit);
    const snapshot = resolveBidSnapshot({
      step: "submitting",
      trackCtx: inputFor(submitBid, track).trackCtx,
      tenderId: TENDER_ID,
      submitBid,
      track,
    });
    const actor = createActor(bidMachine, {
      input: inputFor(submitBid, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.draft.tenderId).toBe(TENDER_ID);
    expect(actor.getSnapshot().context.draft.budget).toBe(90000);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submitBid).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveBidSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      tenderId: TENDER_ID,
      track,
    });
    const actor = createActor(bidMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(BID_EVENTS.submitAttempted);
  });
});

describe("bidMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
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
    expect(ends.has("funding")).toBe(true);
    expect(ends.has("general")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching submitting passes through the full step sequence", () => {
    const paths = getShortestPaths(bidMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("CONTINUE");
    expect(events).toContain("SET_FUNDING");
    expect(events).toContain("NEXT");
    expect(events).toContain("SUBMIT");
  });
});

describe("bidMachine \u{2014} telemetry events (happy path)", () => {
  it("full flow fires the complete funnel in order", async () => {
    const track = vi.fn();
    const actor = createActor(bidMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "CONTINUE" });
    expect(actor.getSnapshot().matches("funding")).toBe(true);

    actor.send({ type: "SET_FUNDING", budget: 90000, duration: 4 });
    expect(actor.getSnapshot().matches("general")).toBe(true);

    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(BID_EVENTS.started);
    expect(events).toContain(BID_EVENTS.fundingSet);
    expect(events).toContain(BID_EVENTS.stepAdvanced);
    expect(events).toContain(BID_EVENTS.submitAttempted);
    expect(events).toContain(BID_EVENTS.submitted);

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
  });

  it("BACK steps return without re-firing forward telemetry", () => {
    const track = vi.fn();
    const actor = createActor(bidMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "CONTINUE" });
    actor.send({ type: "SET_FUNDING", budget: 1000, duration: 1 });
    expect(actor.getSnapshot().matches("general")).toBe(true);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("funding")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("parents")).toBe(true);

    const started = track.mock.calls.filter((c) => c[0] === BID_EVENTS.started);
    expect(started.length).toBe(1);
  });

  it("one step-advanced event fires (general->review)", () => {
    const track = vi.fn();
    const actor = createActor(bidMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "CONTINUE" });
    actor.send({ type: "SET_FUNDING", budget: 5000, duration: 3 });
    actor.send({ type: "NEXT" });

    const advanced = track.mock.calls.filter((c) => c[0] === BID_EVENTS.stepAdvanced);
    expect(advanced.length).toBe(1);
    expect((advanced[0][1] as { to: string }).to).toBe("review");
  });
});

describe("bidMachine \u{2014} submit failure + retry", () => {
  it("submit error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submitBid: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance api unreachable");
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

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(BID_EVENTS.submitted);
  });

  it("submit error -> BACK returns to review without submitting", async () => {
    const track = vi.fn();
    const actor = createActor(bidMachine, {
      input: inputFor(failSubmit, track),
    }).start();

    actor.send({ type: "CONTINUE" });
    actor.send({ type: "SET_FUNDING", budget: 90000, duration: 4 });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });
});

describe("failClosedSubmit", () => {
  it("fails closed instead of fabricating a bid id", async () => {
    await expect(
      failClosedSubmit({ tenderId: TENDER_ID, budget: 1000, duration: 2 }),
    ).rejects.toThrow("bid submission unavailable: DAO governance signer not configured");
  });
});
