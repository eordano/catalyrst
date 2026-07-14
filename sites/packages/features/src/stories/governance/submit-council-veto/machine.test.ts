import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  submitCouncilVetoMachine,
  COUNCIL_VETO_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveCouncilVetoSnapshot,
  slugToState,
  stateToSlug,
  defaultCreate,
  type CreateFn,
  type TrackFn,
} from "./machine";
import type { CreatedProposal } from "@data/lib/catalyst/governance/submit-council-veto";

const RESULT: CreatedProposal = {
  id: "00000000-0000-0000-0000-000000000abc",
  type: "council_decision_veto",
  decision_snapshot_id: "0xsample",
};

const okCreate: CreateFn = async () => RESULT;

const DECISION_URL =
  "https://snapshot.org/#/dao-council.dcl.eth/proposal/0xabc1234567890";

function inputFor(create: CreateFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-council-veto",
      variant: "wizard",
      experimentKey: "gv_council_veto_wizard",
    },
    create,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "details",
  "reasons",
  "coauthors",
  "review",
  "submitting",
  "success",
  "error",
]);

describe("submitCouncilVetoMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(submitCouncilVetoMachine.states));
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

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.details);
    expect(slugToState(null)).toBe("details");
    expect(slugToState(undefined)).toBe("details");
    expect(slugToState("")).toBe("details");
    expect(slugToState("nope")).toBe("details");
    expect(slugToState("reasons")).toBe("reasons");
    expect(slugToState("coauthors")).toBe("coauthors");
    expect(slugToState("review")).toBe("review");
    expect(slugToState("submitting")).toBe("submitting");
    expect(slugToState("success")).toBe("success");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("submitCouncilVetoMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveCouncilVetoSnapshot({
      step: "details",
      trackCtx: inputFor(okCreate, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const create = vi.fn(okCreate);
    const snapshot = resolveCouncilVetoSnapshot({
      step: "submitting",
      trackCtx: inputFor(create, track).trackCtx,
      create,
      track,
    });
    const actor = createActor(submitCouncilVetoMachine, {
      input: inputFor(create, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("hydrating review does NOT fire trackReviewReached (no entry-action replay)", () => {
    const track = vi.fn();
    const snapshot = resolveCouncilVetoSnapshot({
      step: "review",
      trackCtx: inputFor(okCreate, track).trackCtx,
      track,
    });
    const actor = createActor(submitCouncilVetoMachine, {
      input: inputFor(okCreate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveCouncilVetoSnapshot({
      step: "coauthors",
      trackCtx: inputFor(okCreate, track).trackCtx,
      track,
    });
    const actor = createActor(submitCouncilVetoMachine, {
      input: inputFor(okCreate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("coauthors")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "FILL_COAUTHORS", coAuthors: [] });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(COUNCIL_VETO_EVENTS.coauthorsSet);
    expect(events).toContain(COUNCIL_VETO_EVENTS.reviewReached);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "FILL_DETAILS" as const, decisionUrl: DECISION_URL },
  { type: "URL_INVALID" as const },
  { type: "FILL_REASONS" as const, reasons: "x".repeat(40), suggestions: "" },
  { type: "FILL_COAUTHORS" as const, coAuthors: [] },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("submitCouncilVetoMachine \u{2014} model-based path coverage", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(submitCouncilVetoMachine, {
      input: inputFor(okCreate, () => {}),
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
    expect(ends.has("reasons")).toBe(true);
    expect(ends.has("coauthors")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching review passes through every funnel event", () => {
    const paths = getShortestPaths(submitCouncilVetoMachine, {
      input: inputFor(okCreate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("FILL_DETAILS");
    expect(events).toContain("FILL_REASONS");
    expect(events).toContain("FILL_COAUTHORS");
  });
});

describe("submitCouncilVetoMachine \u{2014} telemetry (happy path)", () => {
  it("details -> reasons -> coauthors -> review -> submit -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(submitCouncilVetoMachine, {
      input: inputFor(okCreate, track),
    }).start();

    actor.send({ type: "FILL_DETAILS", decisionUrl: DECISION_URL });
    expect(actor.getSnapshot().matches("reasons")).toBe(true);

    actor.send({ type: "FILL_REASONS", reasons: "x".repeat(40), suggestions: "y".repeat(30) });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);

    actor.send({ type: "FILL_COAUTHORS", coAuthors: ["0x" + "1".repeat(40)] });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(COUNCIL_VETO_EVENTS.started);
    expect(events).toContain(COUNCIL_VETO_EVENTS.reasonsFilled);
    expect(events).toContain(COUNCIL_VETO_EVENTS.coauthorsSet);
    expect(events).toContain(COUNCIL_VETO_EVENTS.reviewReached);
    expect(events).toContain(COUNCIL_VETO_EVENTS.submitting);
    expect(events).toContain(COUNCIL_VETO_EVENTS.submitted);

    expect(events.indexOf(COUNCIL_VETO_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(COUNCIL_VETO_EVENTS.submitted),
    );

    const reasonsCall = track.mock.calls.find((c) => c[0] === COUNCIL_VETO_EVENTS.reasonsFilled);
    expect(reasonsCall?.[1]).toMatchObject({ has_suggestions: true });

    const startedCall = track.mock.calls.find((c) => c[0] === COUNCIL_VETO_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_council_veto_wizard",
      variant: "wizard",
    });
    const submittedCall = track.mock.calls.find((c) => c[0] === COUNCIL_VETO_EVENTS.submitted);
    expect(submittedCall?.[1]).toMatchObject({ proposal_id: RESULT.id });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("the invalid-URL guardrail event fires without advancing", () => {
    const track = vi.fn();
    const actor = createActor(submitCouncilVetoMachine, {
      input: inputFor(okCreate, track),
    }).start();

    actor.send({ type: "URL_INVALID" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(COUNCIL_VETO_EVENTS.urlInvalid);
    expect(events).not.toContain(COUNCIL_VETO_EVENTS.started);
  });

  it("optional suggestions left empty reports has_suggestions:false", () => {
    const track = vi.fn();
    const actor = createActor(submitCouncilVetoMachine, {
      input: inputFor(okCreate, track),
    }).start();

    actor.send({ type: "FILL_DETAILS", decisionUrl: DECISION_URL });
    actor.send({ type: "FILL_REASONS", reasons: "x".repeat(40) });
    const reasonsCall = track.mock.calls.find((c) => c[0] === COUNCIL_VETO_EVENTS.reasonsFilled);
    expect(reasonsCall?.[1]).toMatchObject({ has_suggestions: false });
  });
});

describe("submitCouncilVetoMachine \u{2014} submit failure + retry", () => {
  it("submit error fires gv_council_veto_submit_error and RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const create: CreateFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance unreachable");
      return okCreate(args);
    };

    const actor = createActor(submitCouncilVetoMachine, {
      input: inputFor(create, track),
    }).start();

    actor.send({ type: "FILL_DETAILS", decisionUrl: DECISION_URL });
    actor.send({ type: "FILL_REASONS", reasons: "x".repeat(40) });
    actor.send({ type: "FILL_COAUTHORS", coAuthors: [] });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("governance unreachable");
    expect(track.mock.calls.map((c) => c[0])).toContain(COUNCIL_VETO_EVENTS.submitError);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(COUNCIL_VETO_EVENTS.submitted);
  });
});

describe("defaultCreate", () => {
  it("fails closed instead of fabricating a proposal id", async () => {
    await expect(
      defaultCreate({
        details: { decisionUrl: DECISION_URL },
        reasons: { reasons: "x".repeat(40), suggestions: "" },
        coAuthors: [],
      }),
    ).rejects.toThrow(
      "council veto submission unavailable: DAO governance signer not configured",
    );
  });
});
