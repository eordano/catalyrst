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
const failSubmit: SubmitFn = async () => {
  throw new Error("governance api unreachable");
};

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
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(draftMachine.states));
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
      "intro",
      "details",
      "coauthors",
      "review",
      "submitting",
      "success",
    ]) {
      expect(EXPECTED_STATES.has(slugToState(step))).toBe(true);
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.intro);
    expect(slugToState(null)).toBe("intro");
    expect(slugToState(undefined)).toBe("intro");
    expect(slugToState("")).toBe("intro");
    expect(slugToState("nope")).toBe("intro");
    expect(slugToState("details")).toBe("details");
    expect(slugToState("submit-error")).toBe("submitError");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("draftMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveDraftSnapshot({
      step: "intro",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submitDraft = vi.fn(okSubmit);
    const snapshot = resolveDraftSnapshot({
      step: "submitting",
      trackCtx: inputFor(submitDraft, track).trackCtx,
      submitDraft,
      track,
    });
    const actor = createActor(draftMachine, {
      input: inputFor(submitDraft, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.draft.pollId).toBe("sample-poll");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submitDraft).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveDraftSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      track,
    });
    const actor = createActor(draftMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(
      DRAFT_EVENTS.submitAttempted,
    );
  });
});

describe("draftMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
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
    expect(ends.has("details")).toBe(true);
    expect(ends.has("coauthors")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching submitting passes through the full step sequence", () => {
    const paths = getShortestPaths(draftMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("CLEAR_GATE");
    expect(events).toContain("SUBMIT_DETAILS");
    expect(events).toContain("NEXT");
    expect(events).toContain("SUBMIT");
  });
});

describe("draftMachine \u{2014} telemetry events (happy path)", () => {
  it("full flow fires the complete funnel in order", async () => {
    const track = vi.fn();
    const actor = createActor(draftMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "CLEAR_GATE", pollId: "poll-1" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send({ type: "SUBMIT_DETAILS", title: "A descriptive title", bodies: SAMPLE_BODIES });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);

    actor.send({ type: "NEXT", coauthors: [{ addr: "0xabc\u{2026}123" }] });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DRAFT_EVENTS.started);
    expect(events).toContain(DRAFT_EVENTS.detailsCompleted);
    expect(events).toContain(DRAFT_EVENTS.coauthorsSet);
    expect(events).toContain(DRAFT_EVENTS.stepAdvanced);
    expect(events).toContain(DRAFT_EVENTS.submitAttempted);
    expect(events).toContain(DRAFT_EVENTS.submitted);

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

  it("details_completed carries the title length + body count", () => {
    const track = vi.fn();
    const actor = createActor(draftMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "CLEAR_GATE", pollId: "poll-1" });
    actor.send({ type: "SUBMIT_DETAILS", title: "Hello world", bodies: SAMPLE_BODIES });

    const call = track.mock.calls.find((c) => c[0] === DRAFT_EVENTS.detailsCompleted);
    expect(call?.[1]).toMatchObject({ title_len: 11, bodies: 5 });
  });

  it("coauthors_set carries the chip count", () => {
    const track = vi.fn();
    const actor = createActor(draftMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "CLEAR_GATE", pollId: "poll-1" });
    actor.send({ type: "SUBMIT_DETAILS", title: "Title", bodies: SAMPLE_BODIES });
    actor.send({ type: "NEXT", coauthors: [{ addr: "0xa" }, { addr: "0xb" }] });

    const call = track.mock.calls.find((c) => c[0] === DRAFT_EVENTS.coauthorsSet);
    expect(call?.[1]).toMatchObject({ count: 2 });
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
  it("submit error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submitDraft: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance api unreachable");
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

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(DRAFT_EVENTS.submitted);
  });

  it("submit error -> BACK returns to review without submitting", async () => {
    const track = vi.fn();
    const actor = createActor(draftMachine, {
      input: inputFor(failSubmit, track),
    }).start();

    actor.send({ type: "CLEAR_GATE", pollId: "poll-1" });
    actor.send({ type: "SUBMIT_DETAILS", title: "Title", bodies: SAMPLE_BODIES });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });
});

describe("simulateSubmit", () => {
  it("resolves a stub proposal id keyed by poll (no network)", async () => {
    const r = await simulateSubmit({ pollId: "abcdef1234", title: "Title" });
    expect(r.proposalId).toContain("stub-draft-abcdef12");
  });
});
