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
const failSubmit: SubmitFn = async () => {
  throw new Error("governance api unreachable");
};

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

const EXPECTED_STATES = new Set([
  "intro",
  "details",
  "coauthors",
  "review",
  "submitting",
  "success",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "PASS_GATE" as const },
  { type: "SUBMIT_DETAILS" as const, details: VALID_DETAILS },
  { type: "SUBMIT_COAUTHORS" as const, coAuthors: [] },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("pitchMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(pitchMachine.states));
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
    expect(slugToState("coauthors")).toBe("coauthors");
    expect(slugToState("submitting")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("pitchMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolvePitchSnapshot({
      step: "intro",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
      meetsGate: true,
      votingPower: 12480,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const snapshot = resolvePitchSnapshot({
      step: "submitting",
      trackCtx: inputFor(submit, track).trackCtx,
      meetsGate: true,
      votingPower: 12480,
      submit,
      track,
      draft: validDraft(),
    });
    const actor = createActor(pitchMachine, {
      input: inputFor(submit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.draft.initiative_name).toBe(
      VALID_DETAILS.initiative_name,
    );

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolvePitchSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      meetsGate: true,
      votingPower: 12480,
      track,
      draft: validDraft(),
    });
    const actor = createActor(pitchMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(PITCH_EVENTS.submitting);
  });
});

describe("pitchMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(pitchMachine, {
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
    const paths = getShortestPaths(pitchMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("PASS_GATE");
    expect(events).toContain("SUBMIT_DETAILS");
    expect(events).toContain("SUBMIT_COAUTHORS");
    expect(events).toContain("CONFIRM");
  });
});

describe("pitchMachine \u{2014} VP submission gate", () => {
  it("an eligible account passes the gate into details", () => {
    const track = vi.fn();
    const actor = createActor(pitchMachine, {
      input: inputFor(okSubmit, track, { meetsGate: true }),
    }).start();

    actor.send({ type: "PASS_GATE" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(PITCH_EVENTS.gatePassed);
  });

  it("a below-threshold account stays locked at intro (PASS_GATE no-op)", () => {
    const track = vi.fn();
    const actor = createActor(pitchMachine, {
      input: inputFor(okSubmit, track, { meetsGate: false, votingPower: 12 }),
    }).start();

    actor.send({ type: "PASS_GATE" });
    expect(actor.getSnapshot().matches("intro")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(PITCH_EVENTS.started);
    expect(events).not.toContain(PITCH_EVENTS.gatePassed);
  });

  it("started carries the gate + vp props", () => {
    const track = vi.fn();
    createActor(pitchMachine, {
      input: inputFor(okSubmit, track, { meetsGate: false, votingPower: 12 }),
    }).start();
    const started = track.mock.calls.find((c) => c[0] === PITCH_EVENTS.started);
    expect(started?.[1]).toMatchObject({ meets_gate: false, vp: 12 });
  });
});

describe("pitchMachine \u{2014} telemetry events (happy path)", () => {
  it("full flow fires the complete funnel in order", async () => {
    const track = vi.fn();
    const actor = createActor(pitchMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "PASS_GATE" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send({ type: "SUBMIT_DETAILS", details: VALID_DETAILS });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);

    actor.send({ type: "SUBMIT_COAUTHORS", coAuthors: [] });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(PITCH_EVENTS.started);
    expect(events).toContain(PITCH_EVENTS.gatePassed);
    expect(events).toContain(PITCH_EVENTS.detailsSubmitted);
    expect(events).toContain(PITCH_EVENTS.coauthorsSet);
    expect(events).toContain(PITCH_EVENTS.reviewReached);
    expect(events).toContain(PITCH_EVENTS.submitting);
    expect(events).toContain(PITCH_EVENTS.submitted);

    expect(events.indexOf(PITCH_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(PITCH_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === PITCH_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_pitch_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("details_submitted carries name length + total body chars", () => {
    const track = vi.fn();
    const actor = createActor(pitchMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "PASS_GATE" });
    actor.send({ type: "SUBMIT_DETAILS", details: VALID_DETAILS });

    const call = track.mock.calls.find((c) => c[0] === PITCH_EVENTS.detailsSubmitted);
    expect(call?.[1]).toMatchObject({
      name_length: VALID_DETAILS.initiative_name.length,
    });
    expect((call?.[1] as { body_chars: number }).body_chars).toBeGreaterThan(0);
  });

  it("coauthors_set carries the count", () => {
    const track = vi.fn();
    const actor = createActor(pitchMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    const co = ["0x" + "a".repeat(40), "0x" + "b".repeat(40)];
    actor.send({ type: "PASS_GATE" });
    actor.send({ type: "SUBMIT_DETAILS", details: VALID_DETAILS });
    actor.send({ type: "SUBMIT_COAUTHORS", coAuthors: co });

    const call = track.mock.calls.find((c) => c[0] === PITCH_EVENTS.coauthorsSet);
    expect(call?.[1]).toMatchObject({ count: 2 });
    expect(actor.getSnapshot().context.draft.coAuthors).toEqual(co);
  });
});

describe("pitchMachine \u{2014} invalid details", () => {
  it("a too-short body stays on details, fires details_invalid, sets errors", () => {
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
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(PITCH_EVENTS.detailsInvalid);
    expect(events).not.toContain(PITCH_EVENTS.detailsSubmitted);
    expect(actor.getSnapshot().context.errors.problem_statement).toBeTruthy();

    const invalid = track.mock.calls.find((c) => c[0] === PITCH_EVENTS.detailsInvalid);
    expect((invalid?.[1] as { fields: string[] }).fields).toContain("problem_statement");
  });

  it("a missing initiative name is rejected", () => {
    const track = vi.fn();
    const actor = createActor(pitchMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "PASS_GATE" });
    actor.send({
      type: "SUBMIT_DETAILS",
      details: { ...VALID_DETAILS, initiative_name: "" },
    });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(actor.getSnapshot().context.errors.initiative_name).toBeTruthy();
  });
});

describe("pitchMachine \u{2014} BACK navigation", () => {
  it("BACK from coauthors and review does not re-fire started", () => {
    const track = vi.fn();
    const actor = createActor(pitchMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "PASS_GATE" });
    actor.send({ type: "SUBMIT_DETAILS", details: VALID_DETAILS });
    actor.send({ type: "SUBMIT_COAUTHORS", coAuthors: [] });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    const started = track.mock.calls.filter((c) => c[0] === PITCH_EVENTS.started);
    expect(started.length).toBe(1);
  });
});

describe("pitchMachine \u{2014} submit failure + retry", () => {
  it("submit error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submit: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance api unreachable");
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
    expect(track.mock.calls.map((c) => c[0])).toContain(PITCH_EVENTS.error);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(PITCH_EVENTS.submitted);
  });

  it("submit error -> BACK returns to review without submitting", async () => {
    const track = vi.fn();
    const actor = createActor(pitchMachine, {
      input: inputFor(failSubmit, track),
    }).start();

    actor.send({ type: "PASS_GATE" });
    actor.send({ type: "SUBMIT_DETAILS", details: VALID_DETAILS });
    actor.send({ type: "SUBMIT_COAUTHORS", coAuthors: [] });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });
});

describe("simulateSubmit", () => {
  it("resolves a synthetic stub proposal id (no network)", async () => {
    const r = await simulateSubmit({ draft: { ...emptyDraft() } });
    expect(r.proposalId).toContain("sim-pitch-");
    expect(r.stub).toBe(true);
  });
});
