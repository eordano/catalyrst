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

const TRAVERSAL_EVENTS = [
  { type: "START" as const },
  { type: "GATE" as const },
  { type: "NEXT" as const },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("tenderMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to parent", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(tenderMachine.states)));
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.parent);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("parent");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("tenderMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots parent without a snapshot, hydrates submitting silently, and only real transitions track", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const trackCtx = inputFor(passSeed, submit, track).trackCtx;
    expect(resolveTenderSnapshot({ step: "parent", trackCtx, seed: passSeed })).toBeUndefined();

    const submitting = createActor(tenderMachine, {
      input: inputFor(passSeed, submit, track),
      snapshot: resolveTenderSnapshot({
        step: "submitting",
        trackCtx,
        seed: passSeed,
        form: { linked_proposal_id: LINKED },
        submit,
        track,
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const details = createActor(tenderMachine, {
      input: inputFor(passSeed, okSubmit, track),
      snapshot: resolveTenderSnapshot({
        step: "details",
        trackCtx,
        seed: passSeed,
        form: FILLED,
        track,
      }),
    }).start();
    expect(details.getSnapshot().matches("details")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    details.send({ type: "NEXT" });
    expect(details.getSnapshot().matches("coauthors")).toBe(true);
    expect(names(track)).toContain(TENDER_EVENTS.detailsFilled);
  });
});

describe("tenderMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("event paths reach details, coauthors, review, submitting and gated; below-threshold VP routes START to gated", () => {
    const paths = getShortestPaths(tenderMachine, {
      input: inputFor(passSeed, okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["details", "coauthors", "review", "submitting", "gated"]) {
      expect(ends.has(s)).toBe(true);
    }
    const review = paths.find((p) => (p.state.value as string) === "review");
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("START");
    expect(events.filter((e) => e === "NEXT").length).toBeGreaterThanOrEqual(2);

    const gated = getShortestPaths(tenderMachine, {
      input: inputFor(gateSeed, okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(gated.some((p) => (p.state.value as string) === "details")).toBe(false);
    expect(gated.some((p) => (p.state.value as string) === "gated")).toBe(true);
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

    const events = names(track);
    for (const e of [
      TENDER_EVENTS.started,
      TENDER_EVENTS.detailsFilled,
      TENDER_EVENTS.coauthorsSet,
      TENDER_EVENTS.reviewReached,
      TENDER_EVENTS.submitting,
      TENDER_EVENTS.submitted,
    ]) {
      expect(events).toContain(e);
    }
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
    expect(names(track)).toContain(TENDER_EVENTS.vpGated);
    expect(names(track)).not.toContain(TENDER_EVENTS.started);
    expect(submit).not.toHaveBeenCalled();
  });
});

describe("tenderMachine \u{2014} submit failure", () => {
  it("an error fires gv_tender_submit_error, RETRY recovers, and the shipped default fails closed", async () => {
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
    expect(names(track)).toContain(TENDER_EVENTS.submitError);
    expect(names(track)).toContain(TENDER_EVENTS.submitted);

    await expect(failClosedCreateTender({ form: {} as never })).rejects.toThrow(/unavailable/i);
  });
});
