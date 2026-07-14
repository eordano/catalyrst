import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  tenderMachine,
  TENDER_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveTenderSnapshot,
  slugToState,
  stateToSlug,
  type SubmitFn,
  type TenderSeed,
  type TrackFn,
} from "./machine";
import {
  failClosedCreateTender,
  type CreatedTender,
} from "@data/lib/catalyst/governance/submit-tender";

const LINKED = "e5f9bc17-a46d-4420-a05c-1c73b46d7be1";

const RESULT: CreatedTender = {
  id: "tender-123",
  type: "tender",
  linked_proposal_id: LINKED,
  pending: true,
};

const okSubmit: SubmitFn = async () => RESULT;
const failSubmit: SubmitFn = async () => {
  throw new Error("governance unreachable");
};

const passSeed: TenderSeed = { votingPower: 12480, threshold: 1000, linkedProposalId: LINKED };
const gateSeed: TenderSeed = { votingPower: 300, threshold: 1000, linkedProposalId: LINKED };

function inputFor(seed: TenderSeed, submit: SubmitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-tender",
      variant: "wizard",
      experimentKey: "gv_tender_wizard",
    },
    seed,
    submit,
    track,
  };
}

const FILLED = {
  project_name: "Unified moderation pipeline",
  summary: "x".repeat(40),
  problem_statement: "x".repeat(40),
  technical_specification: "x".repeat(40),
  use_cases: "x".repeat(40),
  deliverables: "x".repeat(40),
  target_release_quarter: "2026 Q4",
};

const EXPECTED_STATES = new Set([
  "parent",
  "gated",
  "details",
  "coauthors",
  "review",
  "submitting",
  "success",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "START" as const },
  { type: "GATE" as const },
  { type: "NEXT" as const },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("tenderMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(tenderMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.parent);
    expect(slugToState(null)).toBe("parent");
    expect(slugToState(undefined)).toBe("parent");
    expect(slugToState("")).toBe("parent");
    expect(slugToState("nope")).toBe("parent");
    expect(slugToState("details")).toBe("details");
    expect(slugToState("coauthors")).toBe("coauthors");
    expect(slugToState("review")).toBe("review");
    expect(slugToState("submitting")).toBe("submitting");
    expect(slugToState("success")).toBe("success");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("tenderMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveTenderSnapshot({
      step: "parent",
      trackCtx: inputFor(passSeed, okSubmit, () => {}).trackCtx,
      seed: passSeed,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const snapshot = resolveTenderSnapshot({
      step: "submitting",
      trackCtx: inputFor(passSeed, submit, track).trackCtx,
      seed: passSeed,
      form: { linked_proposal_id: LINKED },
      submit,
      track,
    });
    const actor = createActor(tenderMachine, {
      input: inputFor(passSeed, submit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveTenderSnapshot({
      step: "details",
      trackCtx: inputFor(passSeed, okSubmit, track).trackCtx,
      seed: passSeed,
      form: FILLED,
      track,
    });
    const actor = createActor(tenderMachine, {
      input: inputFor(passSeed, okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(TENDER_EVENTS.detailsFilled);
  });
});

describe("tenderMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state (gate met)", () => {
    const paths = getShortestPaths(tenderMachine, {
      input: inputFor(passSeed, okSubmit, () => {}),
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
    expect(ends.has("gated")).toBe(true);
  });

  it("reaching review passes through START, NEXT, NEXT", () => {
    const paths = getShortestPaths(tenderMachine, {
      input: inputFor(passSeed, okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("START");
    expect(events.filter((e) => e === "NEXT").length).toBeGreaterThanOrEqual(2);
  });

  it("below-threshold VP routes START to gated", () => {
    const paths = getShortestPaths(tenderMachine, {
      input: inputFor(gateSeed, okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const reachedDetails = paths.some((p) => (p.state.value as string) === "details");
    expect(reachedDetails).toBe(false);
    const reachedGated = paths.some((p) => (p.state.value as string) === "gated");
    expect(reachedGated).toBe(true);
  });
});

describe("tenderMachine \u{2014} telemetry events (happy path)", () => {
  it("start -> details -> coauthors -> review -> submit -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(tenderMachine, {
      input: inputFor(passSeed, okSubmit, track),
    }).start();

    actor.send({ type: "SET_FORM", patch: FILLED });
    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);

    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(TENDER_EVENTS.started);
    expect(events).toContain(TENDER_EVENTS.detailsFilled);
    expect(events).toContain(TENDER_EVENTS.coauthorsSet);
    expect(events).toContain(TENDER_EVENTS.reviewReached);
    expect(events).toContain(TENDER_EVENTS.submitting);
    expect(events).toContain(TENDER_EVENTS.submitted);

    expect(events.indexOf(TENDER_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(TENDER_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === TENDER_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_tender_wizard",
      variant: "wizard",
    });
    const submittedCall = track.mock.calls.find((c) => c[0] === TENDER_EVENTS.submitted);
    expect(submittedCall?.[1]).toMatchObject({ pending: true, proposal_id: RESULT.id });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("tenderMachine \u{2014} VP gate", () => {
  it("below-threshold START fires gv_tender_vp_gated and does not advance/submit", () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const actor = createActor(tenderMachine, {
      input: inputFor(gateSeed, submit, track),
    }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("gated")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(TENDER_EVENTS.vpGated);
    expect(events).not.toContain(TENDER_EVENTS.started);
    expect(submit).not.toHaveBeenCalled();
  });
});

describe("tenderMachine \u{2014} submit failure + retry", () => {
  it("submit error fires gv_tender_submit_error, RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submit: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance unreachable");
      return okSubmit(args);
    };

    const actor = createActor(tenderMachine, {
      input: inputFor(passSeed, submit, track),
    }).start();

    actor.send({ type: "SET_FORM", patch: FILLED });
    actor.send({ type: "START" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("governance unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(TENDER_EVENTS.submitError);
    expect(events).toContain(TENDER_EVENTS.submitted);
  });

  it("failSubmit always errors (sanity)", async () => {
    await expect(failSubmit({ form: {} as never })).rejects.toThrow("governance unreachable");
  });

  it("the shipped default fails closed instead of fabricating a tender id", async () => {
    await expect(failClosedCreateTender({ form: {} as never })).rejects.toThrow(
      "tender submission unavailable: DAO governance signer not configured",
    );
  });
});
