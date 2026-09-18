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
const BLOCKED_GATE: GateInput = { connected: false, hasVp: false };

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

const TRAVERSAL_EVENTS = [
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("submitPollMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to intro", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(submitPollMachine.states)));
    expect(mapped).toEqual(EXPECTED_STATES);
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.intro);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("intro");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("submitPollMachine \u{2014} validation helpers", () => {
  it("enforces the upstream poll limits, >= 2 non-empty options and 42-char co-authors", () => {
    expect(POLL_LIMITS.title).toEqual({ min: 5, max: 80 });
    expect(POLL_LIMITS.description).toEqual({ min: 20, max: 7000 });
    expect(POLL_LIMITS.choices.min).toBe(2);

    expect(areDetailsValid(EMPTY_DRAFT)).toBe(false);
    expect(areDetailsValid(VALID_DRAFT)).toBe(true);
    expect(areDetailsValid({ ...VALID_DRAFT, title: "hi" })).toBe(false);
    expect(areDetailsValid({ ...VALID_DRAFT, description: "too short" })).toBe(false);

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
  it("boots intro without a snapshot, hydrates review/submitting silently, and only real transitions track", async () => {
    const track = vi.fn();
    const submitPoll = vi.fn(okSubmit);
    expect(
      resolveSubmitPollSnapshot({ step: "intro", trackCtx: TRACK_CTX, gate: OK_GATE }),
    ).toBeUndefined();

    const review = createActor(submitPollMachine, {
      input: inputFor(submitPoll, track),
      snapshot: resolveSubmitPollSnapshot({
        step: "review",
        trackCtx: TRACK_CTX,
        gate: OK_GATE,
        draft: VALID_DRAFT,
        submitPoll,
        track,
      }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submitPoll).not.toHaveBeenCalled();
    expect(review.getSnapshot().matches("review")).toBe(true);

    const submitting = createActor(submitPollMachine, {
      input: inputFor(submitPoll, () => {}),
      snapshot: resolveSubmitPollSnapshot({
        step: "submitting",
        trackCtx: TRACK_CTX,
        gate: OK_GATE,
        draft: VALID_DRAFT,
        submitPoll,
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(submitPoll).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const details = createActor(submitPollMachine, {
      input: inputFor(okSubmit, track),
      snapshot: resolveSubmitPollSnapshot({
        step: "details",
        trackCtx: TRACK_CTX,
        gate: OK_GATE,
        draft: VALID_DRAFT,
        track,
      }),
    }).start();
    expect(details.getSnapshot().matches("details")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    details.send({ type: "NEXT" });
    expect(details.getSnapshot().matches("options")).toBe(true);
    expect(names(track)).toContain(SUBMIT_POLL_EVENTS.detailsCompleted);
  });
});

describe("submitPollMachine \u{2014} model-based path coverage + VP/connect gate", () => {
  it("every event-reachable path ends in an expected state; a blocked gate keeps intro and fires vp_blocked", () => {
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
    for (const s of ["details", "options", "review", "submitting"]) expect(ends.has(s)).toBe(true);
    const review = paths.find((p) => (p.state.value as string) === "review");
    const events = review!.steps.map((s) => s.event.type);
    expect(events.filter((e) => e === "NEXT").length).toBeGreaterThanOrEqual(3);

    const blocked = getShortestPaths(submitPollMachine, {
      input: inputFor(okSubmit, () => {}, BLOCKED_GATE),
      events: TRAVERSAL_EVENTS,
    });
    for (const p of blocked) expect(p.state.value).toBe("intro");

    const disconnected = vi.fn();
    const noWallet = createActor(submitPollMachine, {
      input: inputFor(okSubmit, disconnected, BLOCKED_GATE),
    }).start();
    noWallet.send({ type: "NEXT" });
    expect(noWallet.getSnapshot().matches("intro")).toBe(true);
    expect(names(disconnected)).toContain(SUBMIT_POLL_EVENTS.vpBlocked);
    expect(names(disconnected)).not.toContain(SUBMIT_POLL_EVENTS.started);

    const lowVp = vi.fn();
    const underThreshold = createActor(submitPollMachine, {
      input: inputFor(okSubmit, lowVp, { connected: true, hasVp: false }),
    }).start();
    underThreshold.send({ type: "NEXT" });
    expect(underThreshold.getSnapshot().matches("intro")).toBe(true);
    expect(names(lowVp)).toContain(SUBMIT_POLL_EVENTS.vpBlocked);
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

    const events = names(track);
    for (const e of [
      SUBMIT_POLL_EVENTS.started,
      SUBMIT_POLL_EVENTS.detailsCompleted,
      SUBMIT_POLL_EVENTS.optionsCompleted,
      SUBMIT_POLL_EVENTS.reviewReached,
      SUBMIT_POLL_EVENTS.submitted,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(SUBMIT_POLL_EVENTS.started)).toBeLessThan(
      events.indexOf(SUBMIT_POLL_EVENTS.reviewReached),
    );
    expect(events.indexOf(SUBMIT_POLL_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(SUBMIT_POLL_EVENTS.submitted),
    );

    const submittedCall = track.mock.calls.find((c) => c[0] === SUBMIT_POLL_EVENTS.submitted);
    expect(submittedCall?.[1]).toMatchObject({ stub: true });
    expect(submittedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_submit_poll_flow",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("submitPollMachine \u{2014} invalid steps cannot advance", () => {
  it("invalid details NEXT is a no-op until SET_DETAILS makes them valid", () => {
    const actor = createActor(submitPollMachine, {
      input: inputFor(okSubmit, vi.fn(), OK_GATE, EMPTY_DRAFT),
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
    expect(names(track)).toContain(SUBMIT_POLL_EVENTS.submitted);
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
