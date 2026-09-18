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
  it("covers every state, round-trips uniquely, addresses the audit-spec slugs, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(hiringMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    for (const slug of ["target", "reasons", "review", "submitting", "success"]) {
      expect(SLUG_TO_STATE[slug as keyof typeof SLUG_TO_STATE]).toBeDefined();
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.target);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("target");
    }
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("hiringMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step boots from initial; submitting hydrates without telemetry or auto-submit; review then CONFIRM fires", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const input = inputFor(submit, track);

    expect(
      resolveHiringSnapshot({
        step: "target",
        trackCtx: input.trackCtx,
        request: "add",
        errorCopy: ERROR_COPY,
      }),
    ).toBeUndefined();

    const submitting = createActor(hiringMachine, {
      input,
      snapshot: resolveHiringSnapshot({
        step: "submitting",
        trackCtx: input.trackCtx,
        request: "add",
        errorCopy: ERROR_COPY,
        submit,
        track,
        draft: VALID_DRAFT,
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.request).toBe("add");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const review = createActor(hiringMachine, {
      input,
      snapshot: resolveHiringSnapshot({
        step: "review",
        trackCtx: input.trackCtx,
        request: "add",
        errorCopy: ERROR_COPY,
        track,
        draft: VALID_DRAFT,
      }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    review.send({ type: "CONFIRM" });
    expect(review.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(HIRING_EVENTS.submitting);
  });
});

describe("hiringMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("event paths reach target, reasons, review and submitting, and review needs target + reasons", () => {
    const paths = getShortestPaths(hiringMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["target", "reasons", "review", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }

    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toEqual(expect.arrayContaining(["SUBMIT_TARGET", "SUBMIT_REASONS"]));
  });
});

describe("hiringMachine \u{2014} telemetry events (happy path)", () => {
  it("target -> reasons -> review -> confirm -> submit fires the full funnel and accumulates the draft", async () => {
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
      coAuthors: ["0x" + "a".repeat(40)],
    });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    const draft = actor.getSnapshot().context.draft;
    expect(draft.committee).toBe(VALID_DRAFT.committee);
    expect(draft.address).toBe(VALID_DRAFT.address);
    expect(draft.reasons).toBe(VALID_DRAFT.reasons);
    expect(draft.evidence).toBe(VALID_DRAFT.evidence);
    expect(draft.coAuthors).toHaveLength(1);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        HIRING_EVENTS.started,
        HIRING_EVENTS.targetSubmitted,
        HIRING_EVENTS.reasonsSubmitted,
        HIRING_EVENTS.reviewReached,
        HIRING_EVENTS.submitting,
        HIRING_EVENTS.submitted,
      ]),
    );
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
});

describe("hiringMachine \u{2014} inline validation (guardrail)", () => {
  it("invalid target stays on target with gv_hiring_target_invalid; too-short reasons stay on reasons with inline errors", () => {
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

    actor.send({
      type: "SUBMIT_TARGET",
      committee: VALID_DRAFT.committee,
      address: VALID_DRAFT.address,
    });
    expect(actor.getSnapshot().matches("reasons")).toBe(true);

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
    await expect(defaultSubmit({ request: "add", draft: VALID_DRAFT })).rejects.toThrow(/unavailable/i);
  });
});
