import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import { getSubmitHiringData } from "@data/lib/catalyst/governance/submit-hiring";
import type { CreatedProposal } from "@data/lib/catalyst/governance/submit-hiring";
import {
  hiringMachine,
  HIRING_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveHiringSnapshot,
  slugToState,
  stateToSlug,
  defaultSubmit,
  type SubmitFn,
  type TrackFn,
  type HiringDraft,
} from "./machine";

const ERROR_COPY = getSubmitHiringData().copy.errors;

const RESULT: CreatedProposal = {
  id: "hiring-proposal-test",
  type: "hiring_add",
  request: "add",
};

const okSubmit: SubmitFn = async () => RESULT;

const VALID_DRAFT: HiringDraft = {
  committee: "DAO Council",
  address: "0x06012c8cf97bead5deae237070f9587f8e7a266d",
  reasons:
    "This contributor has shown up for the DAO across multiple seasons and would strengthen the Council.",
  evidence:
    "They authored three accepted governance proposals and have a public track record of milestone delivery.",
  coAuthors: [],
};

function inputFor(submit: SubmitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-hiring",
      variant: "wizard",
      experimentKey: "gv_hiring_wizard",
    },
    request: "add" as const,
    errorCopy: ERROR_COPY,
    submit,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "target",
  "reasons",
  "review",
  "submitting",
  "success",
  "error",
]);

const TRAVERSAL_EVENTS = [
  {
    type: "SUBMIT_TARGET" as const,
    committee: VALID_DRAFT.committee,
    address: VALID_DRAFT.address,
  },
  { type: "SUBMIT_TARGET" as const, committee: "", address: "nope" },
  {
    type: "SUBMIT_REASONS" as const,
    reasons: VALID_DRAFT.reasons,
    evidence: VALID_DRAFT.evidence,
    coAuthors: [] as string[],
  },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("hiringMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(hiringMachine.states));
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

  it("the audit-spec step slugs are all addressable", () => {
    for (const slug of ["target", "reasons", "review", "submitting", "success"]) {
      expect(SLUG_TO_STATE[slug as keyof typeof SLUG_TO_STATE]).toBeDefined();
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.target);
    expect(slugToState(null)).toBe("target");
    expect(slugToState(undefined)).toBe("target");
    expect(slugToState("")).toBe("target");
    expect(slugToState("nope")).toBe("target");
    expect(slugToState("reasons")).toBe("reasons");
    expect(slugToState("review")).toBe("review");
    expect(slugToState("submitting")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("hiringMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveHiringSnapshot({
      step: "target",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
      request: "add",
      errorCopy: ERROR_COPY,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step fires NO telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const snapshot = resolveHiringSnapshot({
      step: "submitting",
      trackCtx: inputFor(submit, track).trackCtx,
      request: "add",
      errorCopy: ERROR_COPY,
      submit,
      track,
      draft: VALID_DRAFT,
    });
    const actor = createActor(hiringMachine, {
      input: inputFor(submit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.request).toBe("add");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveHiringSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      request: "add",
      errorCopy: ERROR_COPY,
      track,
      draft: VALID_DRAFT,
    });
    const actor = createActor(hiringMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(HIRING_EVENTS.submitting);
  });
});

describe("hiringMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(hiringMachine, {
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
    expect(ends.has("target")).toBe(true);
    expect(ends.has("reasons")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching review passes through target + reasons", () => {
    const paths = getShortestPaths(hiringMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("SUBMIT_TARGET");
    expect(events).toContain("SUBMIT_REASONS");
  });
});

describe("hiringMachine \u{2014} telemetry events (happy path)", () => {
  it("target -> reasons -> review -> confirm -> submit fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(hiringMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    expect(track.mock.calls.map((c) => c[0])).toContain(HIRING_EVENTS.started);

    actor.send({
      type: "SUBMIT_TARGET",
      committee: VALID_DRAFT.committee,
      address: VALID_DRAFT.address,
    });
    expect(actor.getSnapshot().matches("reasons")).toBe(true);

    actor.send({
      type: "SUBMIT_REASONS",
      reasons: VALID_DRAFT.reasons,
      evidence: VALID_DRAFT.evidence,
      coAuthors: [],
    });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(HIRING_EVENTS.started);
    expect(events).toContain(HIRING_EVENTS.targetSubmitted);
    expect(events).toContain(HIRING_EVENTS.reasonsSubmitted);
    expect(events).toContain(HIRING_EVENTS.reviewReached);
    expect(events).toContain(HIRING_EVENTS.submitting);
    expect(events).toContain(HIRING_EVENTS.submitted);

    expect(events.indexOf(HIRING_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(HIRING_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === HIRING_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_hiring_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("the draft is accumulated across steps", () => {
    const actor = createActor(hiringMachine, {
      input: inputFor(okSubmit, vi.fn()),
    }).start();
    actor.send({
      type: "SUBMIT_TARGET",
      committee: VALID_DRAFT.committee,
      address: VALID_DRAFT.address,
    });
    actor.send({
      type: "SUBMIT_REASONS",
      reasons: VALID_DRAFT.reasons,
      evidence: VALID_DRAFT.evidence,
      coAuthors: ["0x" + "a".repeat(40)],
    });
    const draft = actor.getSnapshot().context.draft;
    expect(draft.committee).toBe(VALID_DRAFT.committee);
    expect(draft.address).toBe(VALID_DRAFT.address);
    expect(draft.reasons).toBe(VALID_DRAFT.reasons);
    expect(draft.evidence).toBe(VALID_DRAFT.evidence);
    expect(draft.coAuthors).toHaveLength(1);
  });
});

describe("hiringMachine \u{2014} inline validation (guardrail)", () => {
  it("invalid target stays on the step and emits gv_hiring_target_invalid", () => {
    const track = vi.fn();
    const actor = createActor(hiringMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "SUBMIT_TARGET", committee: "", address: "nope" });
    expect(actor.getSnapshot().matches("target")).toBe(true);
    expect(actor.getSnapshot().context.errors.committee).toBeTruthy();
    expect(actor.getSnapshot().context.errors.address).toBeTruthy();

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(HIRING_EVENTS.targetInvalid);
    expect(events).not.toContain(HIRING_EVENTS.targetSubmitted);
  });

  it("too-short reasons stays on the step and surfaces an inline error", () => {
    const actor = createActor(hiringMachine, {
      input: inputFor(okSubmit, vi.fn()),
    }).start();
    actor.send({
      type: "SUBMIT_TARGET",
      committee: VALID_DRAFT.committee,
      address: VALID_DRAFT.address,
    });
    actor.send({ type: "SUBMIT_REASONS", reasons: "too short", evidence: "also short", coAuthors: [] });
    expect(actor.getSnapshot().matches("reasons")).toBe(true);
    expect(actor.getSnapshot().context.errors.reasons).toBeTruthy();
    expect(actor.getSnapshot().context.errors.evidence).toBeTruthy();
  });
});

describe("hiringMachine \u{2014} submit failure + retry", () => {
  it("submit error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submit: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("createProposal unavailable");
      return okSubmit(args);
    };

    const actor = createActor(hiringMachine, {
      input: inputFor(submit, track),
    }).start();

    actor.send({
      type: "SUBMIT_TARGET",
      committee: VALID_DRAFT.committee,
      address: VALID_DRAFT.address,
    });
    actor.send({
      type: "SUBMIT_REASONS",
      reasons: VALID_DRAFT.reasons,
      evidence: VALID_DRAFT.evidence,
      coAuthors: [],
    });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("createProposal unavailable");
    expect(track.mock.calls.map((c) => c[0])).toContain(HIRING_EVENTS.submitError);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(HIRING_EVENTS.submitted);
  });
});

describe("defaultSubmit", () => {
  it("fails closed instead of fabricating a proposal id", async () => {
    await expect(defaultSubmit({ request: "add", draft: VALID_DRAFT })).rejects.toThrow(
      "hiring proposal submission unavailable: DAO governance signer not configured",
    );
  });
});
