import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  submitPollMachine,
  SUBMIT_POLL_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  POLL_LIMITS,
  EMPTY_DRAFT,
  resolveSubmitPollSnapshot,
  slugToState,
  stateToSlug,
  simulateSubmit,
  areDetailsValid,
  areOptionsValid,
  cleanOptions,
  type GateInput,
  type PollDraft,
  type SubmitFn,
  type SubmitResult,
  type TrackFn,
} from "./machine";

const RESULT: SubmitResult = { proposalRef: "stub:poll:test" };
const OK_GATE: GateInput = { connected: true, hasVp: true };

const okSubmit: SubmitFn = async () => RESULT;

const VALID_DRAFT: PollDraft = {
  title: "Should the DAO fund a quarterly game jam?",
  description:
    "A poll to gauge community sentiment on a recurring community game jam budget.",
  options: ["Yes", "No"],
  coAuthors: [],
};

const TRACK_CTX = {
  sid: "sid-abc",
  story: "governance-submit-poll",
  variant: "wizard",
  experimentKey: "gv_submit_poll_flow",
};

function inputFor(
  submitPoll: SubmitFn,
  track: TrackFn,
  gate: GateInput = OK_GATE,
  draft: PollDraft = VALID_DRAFT,
) {
  return { trackCtx: TRACK_CTX, gate, draft, submitPoll, track };
}

const EXPECTED_STATES = new Set([
  "intro",
  "details",
  "options",
  "review",
  "submitting",
  "success",
  "error",
]);

describe("submitPollMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(submitPollMachine.states));
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

  it("unknown/missing ?step falls back to the first step (intro)", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.intro);
    expect(slugToState(null)).toBe("intro");
    expect(slugToState(undefined)).toBe("intro");
    expect(slugToState("")).toBe("intro");
    expect(slugToState("nope")).toBe("intro");
    expect(slugToState("details")).toBe("details");
    expect(slugToState("options")).toBe("options");
    expect(slugToState("review")).toBe("review");
    expect(slugToState("submitting")).toBe("submitting");
    expect(slugToState("success")).toBe("success");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("submitPollMachine \u{2014} validation helpers", () => {
  it("enforces the upstream poll schema limits", () => {
    expect(POLL_LIMITS.title).toEqual({ min: 5, max: 80 });
    expect(POLL_LIMITS.description).toEqual({ min: 20, max: 7000 });
    expect(POLL_LIMITS.choices.min).toBe(2);

    expect(areDetailsValid(EMPTY_DRAFT)).toBe(false);
    expect(areDetailsValid(VALID_DRAFT)).toBe(true);
    expect(areDetailsValid({ ...VALID_DRAFT, title: "hi" })).toBe(false);
    expect(areDetailsValid({ ...VALID_DRAFT, description: "too short" })).toBe(false);
  });

  it("requires >= 2 non-empty options and 42-char co-authors", () => {
    expect(areOptionsValid({ ...VALID_DRAFT, options: ["Yes"] })).toBe(false);
    expect(areOptionsValid({ ...VALID_DRAFT, options: ["Yes", ""] })).toBe(false);
    expect(areOptionsValid({ ...VALID_DRAFT, options: ["Yes", "No", " "] })).toBe(true);
    expect(cleanOptions(["Yes", "", " No "])).toEqual(["Yes", "No"]);
    expect(areOptionsValid({ ...VALID_DRAFT, coAuthors: ["0xshort"] })).toBe(false);
    expect(
      areOptionsValid({ ...VALID_DRAFT, coAuthors: ["0x" + "a".repeat(40)] }),
    ).toBe(true);
  });
});

describe("submitPollMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveSubmitPollSnapshot({
      step: "intro",
      trackCtx: TRACK_CTX,
      gate: OK_GATE,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating review does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submitPoll = vi.fn(okSubmit);
    const snapshot = resolveSubmitPollSnapshot({
      step: "review",
      trackCtx: TRACK_CTX,
      gate: OK_GATE,
      draft: VALID_DRAFT,
      submitPoll,
      track,
    });
    const actor = createActor(submitPollMachine, {
      input: inputFor(submitPoll, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submitPoll).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });

  it("hydrating submitting does NOT auto-start the simulated submit", async () => {
    const submitPoll = vi.fn(okSubmit);
    const snapshot = resolveSubmitPollSnapshot({
      step: "submitting",
      trackCtx: TRACK_CTX,
      gate: OK_GATE,
      draft: VALID_DRAFT,
      submitPoll,
    });
    const actor = createActor(submitPollMachine, {
      input: inputFor(submitPoll, () => {}),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(submitPoll).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("a real transition after hydration still fires telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveSubmitPollSnapshot({
      step: "details",
      trackCtx: TRACK_CTX,
      gate: OK_GATE,
      draft: VALID_DRAFT,
      track,
    });
    const actor = createActor(submitPollMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("options")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(
      SUBMIT_POLL_EVENTS.detailsCompleted,
    );
  });
});

const TRAVERSAL_EVENTS = [
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

describe("submitPollMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(submitPollMachine, {
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
    expect(ends.has("details")).toBe(true);
    expect(ends.has("options")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching review passes through intro -> details -> options NEXTs", () => {
    const paths = getShortestPaths(submitPollMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events.filter((e) => e === "NEXT").length).toBeGreaterThanOrEqual(3);
  });

  it("a blocked gate keeps the machine on intro (no advance)", () => {
    const paths = getShortestPaths(submitPollMachine, {
      input: inputFor(okSubmit, () => {}, { connected: false, hasVp: false }),
      events: TRAVERSAL_EVENTS,
    });
    for (const p of paths) {
      expect(p.state.value).toBe("intro");
    }
  });
});

describe("submitPollMachine \u{2014} telemetry (happy path)", () => {
  it("intro -> details -> options -> review -> submit -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(submitPollMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("options")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(SUBMIT_POLL_EVENTS.started);
    expect(events).toContain(SUBMIT_POLL_EVENTS.detailsCompleted);
    expect(events).toContain(SUBMIT_POLL_EVENTS.optionsCompleted);
    expect(events).toContain(SUBMIT_POLL_EVENTS.reviewReached);
    expect(events).toContain(SUBMIT_POLL_EVENTS.submitted);

    expect(events.indexOf(SUBMIT_POLL_EVENTS.started)).toBeLessThan(
      events.indexOf(SUBMIT_POLL_EVENTS.reviewReached),
    );
    expect(events.indexOf(SUBMIT_POLL_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(SUBMIT_POLL_EVENTS.submitted),
    );

    const submittedCall = track.mock.calls.find(
      (c) => c[0] === SUBMIT_POLL_EVENTS.submitted,
    );
    expect(submittedCall?.[1]).toMatchObject({ stub: true });
    expect(submittedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_submit_poll_flow",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("submitPollMachine \u{2014} VP/connect gate (guardrail)", () => {
  it("a disconnected wallet blocks NEXT and fires gv_submit_poll_vp_blocked", () => {
    const track = vi.fn();
    const actor = createActor(submitPollMachine, {
      input: inputFor(okSubmit, track, { connected: false, hasVp: false }),
    }).start();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("intro")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(SUBMIT_POLL_EVENTS.vpBlocked);
    expect(events).not.toContain(SUBMIT_POLL_EVENTS.started);
  });

  it("connected but under 100 VP also blocks", () => {
    const track = vi.fn();
    const actor = createActor(submitPollMachine, {
      input: inputFor(okSubmit, track, { connected: true, hasVp: false }),
    }).start();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("intro")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(SUBMIT_POLL_EVENTS.vpBlocked);
  });
});

describe("submitPollMachine \u{2014} invalid steps cannot advance", () => {
  it("invalid details NEXT is a no-op (stays on details)", () => {
    const track = vi.fn();
    const actor = createActor(submitPollMachine, {
      input: inputFor(okSubmit, track, OK_GATE, EMPTY_DRAFT),
    }).start();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send({
      type: "SET_DETAILS",
      title: "A valid poll title here",
      description: "A description that is comfortably over twenty characters long.",
    });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("options")).toBe(true);
  });
});

describe("submitPollMachine \u{2014} submit failure + retry", () => {
  it("submit error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submitPoll: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("snapshot unreachable");
      return okSubmit(args);
    };
    const actor = createActor(submitPollMachine, {
      input: inputFor(submitPoll, track),
    }).start();

    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("snapshot unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(SUBMIT_POLL_EVENTS.submitted);
  });
});

describe("simulateSubmit", () => {
  it("resolves a deterministic stub proposal ref (no network)", async () => {
    const r = await simulateSubmit({ draft: VALID_DRAFT });
    expect(r.proposalRef.startsWith("stub:poll:")).toBe(true);
    const r2 = await simulateSubmit({ draft: VALID_DRAFT });
    expect(r2.proposalRef).toBe(r.proposalRef);
  });
});
