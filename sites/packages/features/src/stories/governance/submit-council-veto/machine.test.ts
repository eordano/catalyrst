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

const TRAVERSAL_EVENTS = [
  { type: "FILL_DETAILS" as const, decisionUrl: DECISION_URL },
  { type: "URL_INVALID" as const },
  { type: "FILL_REASONS" as const, reasons: "x".repeat(40), suggestions: "" },
  { type: "FILL_COAUTHORS" as const, coAuthors: [] },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("submitCouncilVetoMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(submitCouncilVetoMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.details);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("details");
    }
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("submitCouncilVetoMachine \u{2014} deep-link hydration", () => {
  it("first step boots from initial; later steps hydrate silently; only real transitions fire telemetry", async () => {
    const track = vi.fn();
    const create = vi.fn(okCreate);
    const input = inputFor(create, track);

    expect(
      resolveCouncilVetoSnapshot({ step: "details", trackCtx: input.trackCtx }),
    ).toBeUndefined();

    const submitting = createActor(submitCouncilVetoMachine, {
      input,
      snapshot: resolveCouncilVetoSnapshot({
        step: "submitting",
        trackCtx: input.trackCtx,
        create,
        track,
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const review = createActor(submitCouncilVetoMachine, {
      input,
      snapshot: resolveCouncilVetoSnapshot({ step: "review", trackCtx: input.trackCtx, track }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    const coauthors = createActor(submitCouncilVetoMachine, {
      input,
      snapshot: resolveCouncilVetoSnapshot({ step: "coauthors", trackCtx: input.trackCtx, track }),
    }).start();
    expect(coauthors.getSnapshot().matches("coauthors")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    coauthors.send({ type: "FILL_COAUTHORS", coAuthors: [] });
    expect(coauthors.getSnapshot().matches("review")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(COUNCIL_VETO_EVENTS.coauthorsSet);
    expect(events).toContain(COUNCIL_VETO_EVENTS.reviewReached);
  });
});

describe("submitCouncilVetoMachine \u{2014} model-based path coverage", () => {
  it("event paths reach details, reasons, coauthors, review and submitting, and review needs every funnel event", () => {
    const paths = getShortestPaths(submitCouncilVetoMachine, {
      input: inputFor(okCreate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["details", "reasons", "coauthors", "review", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }

    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toEqual(
      expect.arrayContaining(["FILL_DETAILS", "FILL_REASONS", "FILL_COAUTHORS"]),
    );
  });
});

describe("submitCouncilVetoMachine \u{2014} happy path", () => {
  it("an invalid URL stays on details, then the full funnel fires in order through success", async () => {
    const track = vi.fn();
    const actor = createActor(submitCouncilVetoMachine, {
      input: inputFor(okCreate, track),
    }).start();

    actor.send({ type: "URL_INVALID" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(COUNCIL_VETO_EVENTS.urlInvalid);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(COUNCIL_VETO_EVENTS.started);

    actor.send({ type: "FILL_DETAILS", decisionUrl: DECISION_URL });
    expect(actor.getSnapshot().matches("reasons")).toBe(true);

    actor.send({ type: "FILL_REASONS", reasons: "x".repeat(40), suggestions: "y".repeat(30) });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);

    actor.send({ type: "FILL_COAUTHORS", coAuthors: ["0x" + "1".repeat(40)] });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        COUNCIL_VETO_EVENTS.started,
        COUNCIL_VETO_EVENTS.reasonsFilled,
        COUNCIL_VETO_EVENTS.coauthorsSet,
        COUNCIL_VETO_EVENTS.reviewReached,
        COUNCIL_VETO_EVENTS.submitting,
        COUNCIL_VETO_EVENTS.submitted,
      ]),
    );
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
});

describe("submitCouncilVetoMachine \u{2014} submit failure + retry", () => {
  it("empty suggestions report has_suggestions:false; submit error fires submit_error and RETRY recovers", async () => {
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
    const reasonsCall = track.mock.calls.find((c) => c[0] === COUNCIL_VETO_EVENTS.reasonsFilled);
    expect(reasonsCall?.[1]).toMatchObject({ has_suggestions: false });

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
    ).rejects.toThrow(/unavailable/i);
  });
});
