import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  pitchMachine,
  PITCH_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolvePitchSnapshot,
  slugToState,
  stateToSlug,
  simulateSubmit,
  emptyDraft,
  type SubmitFn,
  type SubmitResult,
  type TrackFn,
  type PitchDraft,
} from "./machine";
import type { PitchDetails } from "@data/lib/catalyst/governance/submit-pitch";

const RESULT: SubmitResult = { proposalId: "sim-pitch-abc", stub: true };

const okSubmit: SubmitFn = async () => RESULT;

const VALID_DETAILS: PitchDetails = {
  initiative_name: "DAO mobile companion app",
  problem_statement: "Decentraland is desktop-first and has no mobile companion.",
  proposed_solution: "Fund a small team to ship a read-first mobile companion app.",
  target_audience: "Active community members and DAO voters who attend events.",
  relevance: "Mobile engagement is where competing platforms capture attention.",
};

function validDraft(): PitchDraft {
  return { ...VALID_DETAILS, coAuthors: [] };
}

function inputFor(
  submit: SubmitFn,
  track: TrackFn,
  opts: { meetsGate?: boolean; votingPower?: number } = {},
) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-pitch",
      variant: "wizard",
      experimentKey: "gv_pitch_wizard",
    },
    meetsGate: opts.meetsGate ?? true,
    votingPower: opts.votingPower ?? 12480,
    submit,
    track,
  };
}

const TRAVERSAL_EVENTS = [
  { type: "PASS_GATE" as const },
  { type: "SUBMIT_DETAILS" as const, details: VALID_DETAILS },
  { type: "SUBMIT_COAUTHORS" as const, coAuthors: [] },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("pitchMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to intro", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(pitchMachine.states)));
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

describe("pitchMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots intro without a snapshot, hydrates later steps silently, and only real transitions track", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const trackCtx = inputFor(submit, track).trackCtx;
    expect(
      resolvePitchSnapshot({ step: "intro", trackCtx, meetsGate: true, votingPower: 12480 }),
    ).toBeUndefined();

    const submitting = createActor(pitchMachine, {
      input: inputFor(submit, track),
      snapshot: resolvePitchSnapshot({
        step: "submitting",
        trackCtx,
        meetsGate: true,
        votingPower: 12480,
        submit,
        track,
        draft: validDraft(),
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.draft.initiative_name).toBe(
      VALID_DETAILS.initiative_name,
    );
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const review = createActor(pitchMachine, {
      input: inputFor(okSubmit, track),
      snapshot: resolvePitchSnapshot({
        step: "review",
        trackCtx,
        meetsGate: true,
        votingPower: 12480,
        track,
        draft: validDraft(),
      }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    review.send({ type: "CONFIRM" });
    expect(review.getSnapshot().matches("submitting")).toBe(true);
    expect(names(track)).toContain(PITCH_EVENTS.submitting);
  });
});

describe("pitchMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("event paths reach details, coauthors, review and submitting, and submitting needs the full step sequence", () => {
    const paths = getShortestPaths(pitchMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["details", "coauthors", "review", "submitting"]) expect(ends.has(s)).toBe(true);

    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    const events = submitting!.steps.map((s) => s.event.type);
    for (const e of ["PASS_GATE", "SUBMIT_DETAILS", "SUBMIT_COAUTHORS", "CONFIRM"]) {
      expect(events).toContain(e);
    }
  });
});

describe("pitchMachine \u{2014} VP submission gate", () => {
  it("only an eligible account passes into details; started carries the gate + vp props", () => {
    const passTrack = vi.fn();
    const eligible = createActor(pitchMachine, {
      input: inputFor(okSubmit, passTrack, { meetsGate: true }),
    }).start();
    eligible.send({ type: "PASS_GATE" });
    expect(eligible.getSnapshot().matches("details")).toBe(true);
    expect(names(passTrack)).toContain(PITCH_EVENTS.gatePassed);

    const lockTrack = vi.fn();
    const locked = createActor(pitchMachine, {
      input: inputFor(okSubmit, lockTrack, { meetsGate: false, votingPower: 12 }),
    }).start();
    locked.send({ type: "PASS_GATE" });
    expect(locked.getSnapshot().matches("intro")).toBe(true);
    expect(names(lockTrack)).toContain(PITCH_EVENTS.started);
    expect(names(lockTrack)).not.toContain(PITCH_EVENTS.gatePassed);
    const started = lockTrack.mock.calls.find((c) => c[0] === PITCH_EVENTS.started);
    expect(started?.[1]).toMatchObject({ meets_gate: false, vp: 12 });
  });
});

describe("pitchMachine \u{2014} happy path", () => {
  it("fires the complete funnel in order with step props, and BACK never re-fires started", async () => {
    const track = vi.fn();
    const actor = createActor(pitchMachine, {
      input: inputFor(okSubmit, track),
    }).start();
    const co = ["0x" + "a".repeat(40), "0x" + "b".repeat(40)];

    actor.send({ type: "PASS_GATE" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    actor.send({ type: "SUBMIT_DETAILS", details: VALID_DETAILS });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);
    actor.send({ type: "SUBMIT_COAUTHORS", coAuthors: co });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(actor.getSnapshot().context.draft.coAuthors).toEqual(co);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(names(track).filter((e) => e === PITCH_EVENTS.started)).toHaveLength(1);

    actor.send({ type: "SUBMIT_DETAILS", details: VALID_DETAILS });
    actor.send({ type: "SUBMIT_COAUTHORS", coAuthors: co });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      PITCH_EVENTS.started,
      PITCH_EVENTS.gatePassed,
      PITCH_EVENTS.detailsSubmitted,
      PITCH_EVENTS.coauthorsSet,
      PITCH_EVENTS.reviewReached,
      PITCH_EVENTS.submitting,
      PITCH_EVENTS.submitted,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(PITCH_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(PITCH_EVENTS.submitted),
    );

    const call = (name: string) => track.mock.calls.find((c) => c[0] === name);
    expect(call(PITCH_EVENTS.started)?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_pitch_wizard",
      variant: "wizard",
    });
    expect(call(PITCH_EVENTS.detailsSubmitted)?.[1]).toMatchObject({
      name_length: VALID_DETAILS.initiative_name.length,
    });
    expect(
      (call(PITCH_EVENTS.detailsSubmitted)?.[1] as { body_chars: number }).body_chars,
    ).toBeGreaterThan(0);
    expect(call(PITCH_EVENTS.coauthorsSet)?.[1]).toMatchObject({ count: 2 });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("pitchMachine \u{2014} invalid details", () => {
  it("a too-short body or missing name stays on details with field errors and fires details_invalid", () => {
    const track = vi.fn();
    const actor = createActor(pitchMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "PASS_GATE" });
    actor.send({
      type: "SUBMIT_DETAILS",
      details: { ...VALID_DETAILS, problem_statement: "too short" },
    });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(names(track)).toContain(PITCH_EVENTS.detailsInvalid);
    expect(names(track)).not.toContain(PITCH_EVENTS.detailsSubmitted);
    expect(actor.getSnapshot().context.errors.problem_statement).toBeTruthy();
    const invalid = track.mock.calls.find((c) => c[0] === PITCH_EVENTS.detailsInvalid);
    expect((invalid?.[1] as { fields: string[] }).fields).toContain("problem_statement");

    actor.send({ type: "SUBMIT_DETAILS", details: { ...VALID_DETAILS, initiative_name: "" } });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(actor.getSnapshot().context.errors.initiative_name).toBeTruthy();
  });
});

describe("pitchMachine \u{2014} submit failure", () => {
  it("an error offers BACK to review and RETRY, which recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submit: SubmitFn = async (args) => {
      calls += 1;
      if (calls < 3) throw new Error("governance api unreachable");
      return okSubmit(args);
    };
    const actor = createActor(pitchMachine, {
      input: inputFor(submit, track),
    }).start();

    actor.send({ type: "PASS_GATE" });
    actor.send({ type: "SUBMIT_DETAILS", details: VALID_DETAILS });
    actor.send({ type: "SUBMIT_COAUTHORS", coAuthors: [] });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("governance api unreachable");
    expect(names(track)).toContain(PITCH_EVENTS.error);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(names(track)).toContain(PITCH_EVENTS.submitted);
  });
});

describe("simulateSubmit", () => {
  it("resolves a synthetic stub proposal id (no network)", async () => {
    const r = await simulateSubmit({ draft: { ...emptyDraft() } });
    expect(r.proposalId).toContain("sim-pitch-");
    expect(r.stub).toBe(true);
  });
});
