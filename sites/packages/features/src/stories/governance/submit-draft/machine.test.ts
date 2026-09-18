import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  draftMachine,
  DRAFT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveDraftSnapshot,
  slugToState,
  stateToSlug,
  simulateSubmit,
  type SubmitFn,
  type SubmitResult,
  type TrackFn,
} from "./machine";

const RESULT: SubmitResult = { proposalId: "stub-draft-abc12345" };

const okSubmit: SubmitFn = async () => RESULT;

const SAMPLE_BODIES = {
  summary: "A one sentence summary of the draft proposal.",
  abstract: "The abstract describing motivation and outcomes.",
  motivation: "Why this is needed.",
  specification: "What the policy proposes.",
  conclusion: "Closing statement.",
};

function inputFor(submitDraft: SubmitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-draft",
      variant: "wizard",
      experimentKey: "gv_draft_wizard",
    },
    submitDraft,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "intro",
  "details",
  "coauthors",
  "review",
  "submitting",
  "submitError",
  "success",
]);

const TRAVERSAL_EVENTS = [
  { type: "CLEAR_GATE" as const, pollId: "poll-1" },
  { type: "SUBMIT_DETAILS" as const, title: "A descriptive title", bodies: SAMPLE_BODIES },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

describe("draftMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, routes the spec steps, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(draftMachine.states));
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
    for (const step of ["intro", "details", "coauthors", "review", "submitting", "success"]) {
      expect(EXPECTED_STATES.has(slugToState(step))).toBe(true);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.intro);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("intro");
    }
    expect(slugToState("submit-error")).toBe("submitError");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("draftMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step boots from initial; submitting hydrates without telemetry or auto-submit; review then SUBMIT fires", async () => {
    const track = vi.fn();
    const submitDraft = vi.fn(okSubmit);
    const input = inputFor(submitDraft, track);

    expect(resolveDraftSnapshot({ step: "intro", trackCtx: input.trackCtx })).toBeUndefined();

    const submitting = createActor(draftMachine, {
      input,
      snapshot: resolveDraftSnapshot({
        step: "submitting",
        trackCtx: input.trackCtx,
        submitDraft,
        track,
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.draft.pollId).toBe("sample-poll");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submitDraft).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const review = createActor(draftMachine, {
      input,
      snapshot: resolveDraftSnapshot({ step: "review", trackCtx: input.trackCtx, track }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    review.send({ type: "SUBMIT" });
    expect(review.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(DRAFT_EVENTS.submitAttempted);
  });
});

describe("draftMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and submitting needs the full step sequence", () => {
    const paths = getShortestPaths(draftMachine, {
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
    for (const s of ["details", "coauthors", "review", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }

    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toEqual(
      expect.arrayContaining(["CLEAR_GATE", "SUBMIT_DETAILS", "NEXT", "SUBMIT"]),
    );
  });
});

describe("draftMachine \u{2014} telemetry events (happy path)", () => {
  it("full flow fires the funnel in order with the details + coauthor payloads", async () => {
    const track = vi.fn();
    const actor = createActor(draftMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "CLEAR_GATE", pollId: "poll-1" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send({ type: "SUBMIT_DETAILS", title: "Hello world", bodies: SAMPLE_BODIES });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);
    const details = track.mock.calls.find((c) => c[0] === DRAFT_EVENTS.detailsCompleted);
    expect(details?.[1]).toMatchObject({ title_len: 11, bodies: 5 });

    actor.send({ type: "NEXT", coauthors: [{ addr: "0xa" }, { addr: "0xb" }] });
    const coauthors = track.mock.calls.find((c) => c[0] === DRAFT_EVENTS.coauthorsSet);
    expect(coauthors?.[1]).toMatchObject({ count: 2 });

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        DRAFT_EVENTS.started,
        DRAFT_EVENTS.detailsCompleted,
        DRAFT_EVENTS.coauthorsSet,
        DRAFT_EVENTS.stepAdvanced,
        DRAFT_EVENTS.submitAttempted,
        DRAFT_EVENTS.submitted,
      ]),
    );
    expect(events.indexOf(DRAFT_EVENTS.submitAttempted)).toBeLessThan(
      events.indexOf(DRAFT_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === DRAFT_EVENTS.started);
    expect(startedCall?.[1]).toMatchObject({ poll_id: "poll-1" });
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_draft_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("BACK steps return without re-firing forward telemetry", () => {
    const track = vi.fn();
    const actor = createActor(draftMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "CLEAR_GATE", pollId: "poll-1" });
    actor.send({ type: "SUBMIT_DETAILS", title: "Title", bodies: SAMPLE_BODIES });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("intro")).toBe(true);

    const started = track.mock.calls.filter((c) => c[0] === DRAFT_EVENTS.started);
    expect(started.length).toBe(1);
  });
});

describe("draftMachine \u{2014} submit failure + retry", () => {
  it("submit error -> BACK returns to review; a second failure -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submitDraft: SubmitFn = async (args) => {
      calls += 1;
      if (calls <= 2) throw new Error("governance api unreachable");
      return okSubmit(args);
    };

    const actor = createActor(draftMachine, {
      input: inputFor(submitDraft, track),
    }).start();

    actor.send({ type: "CLEAR_GATE", pollId: "poll-1" });
    actor.send({ type: "SUBMIT_DETAILS", title: "Title", bodies: SAMPLE_BODIES });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));
    expect(actor.getSnapshot().context.error).toBe("governance api unreachable");

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(calls).toBe(3);
    expect(track.mock.calls.map((c) => c[0])).toContain(DRAFT_EVENTS.submitted);
  });
});

describe("simulateSubmit", () => {
  it("resolves a stub proposal id keyed by poll (no network)", async () => {
    const r = await simulateSubmit({ pollId: "abcdef1234", title: "Title" });
    expect(r.proposalId).toContain("stub-draft-abcdef12");
  });
});
