import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  govProposalMachine,
  GOVPROP_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveGovProposalSnapshot,
  slugToState,
  stateToSlug,
  failClosedSubmit,
  emptyDraft,
  type SubmitFn,
  type SubmitResult,
  type TrackFn,
} from "./machine";
import { GOVERNANCE_SCHEMA } from "@data/lib/catalyst/governance/submit-governance-proposal";

const RESULT: SubmitResult = { id: "govprop-abc", type: "governance" };

const okSubmit: SubmitFn = async () => RESULT;

function validDetails() {
  const bodies: Record<string, string> = {};
  for (const b of GOVERNANCE_SCHEMA.bodies) {
    bodies[b.name] = "This is a sufficiently long body paragraph for the section.";
  }
  return {
    type: "SUBMIT_DETAILS" as const,
    linkedDraftId: "9d0f5b6f-1f47-4371-8a30-4ee99e3792ef",
    title: "Formalize the passed Draft into a binding Governance Proposal",
    bodies,
  };
}

function inputFor(submit: SubmitFn, track: TrackFn, votingPower = 3000) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-governance-proposal",
      variant: "wizard",
      experimentKey: "gv_govprop_wizard",
    },
    votingPower,
    submit,
    track,
  };
}

const TRAVERSAL_EVENTS = [
  { type: "START" as const },
  validDetails(),
  { type: "SET_COAUTHORS" as const, coAuthors: [] },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

describe("govProposalMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, routes the spec steps, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(govProposalMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    for (const step of ["intro", "details", "coauthors", "review", "submitting", "success"]) {
      expect(SLUG_TO_STATE[step as keyof typeof SLUG_TO_STATE]).toBeDefined();
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.intro);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("intro");
    }
    expect(slugToState("submit-error")).toBe("submitError");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("govProposalMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step boots from initial; submitting hydrates without telemetry or auto-submit; review then SUBMIT fires", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const input = inputFor(submit, track);

    expect(
      resolveGovProposalSnapshot({ step: "intro", trackCtx: input.trackCtx, votingPower: 3000 }),
    ).toBeUndefined();

    const submitting = createActor(govProposalMachine, {
      input,
      snapshot: resolveGovProposalSnapshot({
        step: "submitting",
        trackCtx: input.trackCtx,
        votingPower: 3000,
        submit,
        track,
        draft: { title: "T" },
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.draft.title).toBe("T");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const review = createActor(govProposalMachine, {
      input,
      snapshot: resolveGovProposalSnapshot({
        step: "review",
        trackCtx: input.trackCtx,
        votingPower: 3000,
        track,
      }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    review.send({ type: "SUBMIT" });
    expect(review.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(GOVPROP_EVENTS.submitAttempted);
  });
});

describe("govProposalMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("event paths reach details, coauthors, review and submitting, and submitting needs the full step sequence", () => {
    const paths = getShortestPaths(govProposalMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
    }
    for (const s of ["details", "coauthors", "review", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }

    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toEqual(expect.arrayContaining(["START", "SUBMIT_DETAILS", "NEXT", "SUBMIT"]));
  });
});

describe("govProposalMachine \u{2014} telemetry events (happy path)", () => {
  it("full flow fires the complete funnel in order", async () => {
    const track = vi.fn();
    const actor = createActor(govProposalMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("details")).toBe(true);

    actor.send(validDetails());
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);

    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        GOVPROP_EVENTS.started,
        GOVPROP_EVENTS.detailsSubmitted,
        GOVPROP_EVENTS.stepAdvanced,
        GOVPROP_EVENTS.submitAttempted,
        GOVPROP_EVENTS.submitted,
      ]),
    );
    expect(events.indexOf(GOVPROP_EVENTS.submitAttempted)).toBeLessThan(
      events.indexOf(GOVPROP_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === GOVPROP_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_govprop_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("BACK steps return without re-firing forward telemetry", () => {
    const track = vi.fn();
    const actor = createActor(govProposalMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send(validDetails());
    expect(actor.getSnapshot().matches("coauthors")).toBe(true);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("intro")).toBe(true);

    const started = track.mock.calls.filter((c) => c[0] === GOVPROP_EVENTS.started);
    expect(started.length).toBe(1);
  });
});

describe("govProposalMachine \u{2014} VP gate (>=2500 VP)", () => {
  it("START under the threshold stays on intro and logs the guardrail; exactly the threshold advances", () => {
    const blockedTrack = vi.fn();
    const blocked = createActor(govProposalMachine, {
      input: inputFor(okSubmit, blockedTrack, 1000),
    }).start();
    blocked.send({ type: "START" });
    expect(blocked.getSnapshot().matches("intro")).toBe(true);
    const blockedEvents = blockedTrack.mock.calls.map((c) => c[0]);
    expect(blockedEvents).toContain(GOVPROP_EVENTS.vpBlocked);
    expect(blockedEvents).not.toContain(GOVPROP_EVENTS.started);

    const allowedTrack = vi.fn();
    const allowed = createActor(govProposalMachine, {
      input: inputFor(okSubmit, allowedTrack, GOVERNANCE_SCHEMA.vpThreshold),
    }).start();
    allowed.send({ type: "START" });
    expect(allowed.getSnapshot().matches("details")).toBe(true);
    expect(allowedTrack.mock.calls.map((c) => c[0])).toContain(GOVPROP_EVENTS.started);
  });
});

describe("govProposalMachine \u{2014} details validation", () => {
  it("invalid details stay on the step and log the guardrail", () => {
    const track = vi.fn();
    const actor = createActor(govProposalMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send({
      type: "SUBMIT_DETAILS",
      linkedDraftId: "",
      title: "x",
      bodies: {},
    });

    expect(actor.getSnapshot().matches("details")).toBe(true);
    const errs = actor.getSnapshot().context.errors;
    expect(errs.linkedDraftId).toBeTruthy();
    expect(errs.title).toBeTruthy();
    expect(errs.summary).toBeTruthy();
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(GOVPROP_EVENTS.detailsInvalid);
    expect(events).not.toContain(GOVPROP_EVENTS.detailsSubmitted);
  });
});

describe("govProposalMachine \u{2014} submit failure + retry", () => {
  it("submit error -> BACK returns to review; a second failure -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submit: SubmitFn = async (args) => {
      calls += 1;
      if (calls <= 2) throw new Error("governance api unreachable");
      return okSubmit(args);
    };

    const actor = createActor(govProposalMachine, {
      input: inputFor(submit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send(validDetails());
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));
    expect(actor.getSnapshot().context.error).toBe("governance api unreachable");
    expect(track.mock.calls.map((c) => c[0])).toContain(GOVPROP_EVENTS.error);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(calls).toBe(3);
    expect(track.mock.calls.map((c) => c[0])).toContain(GOVPROP_EVENTS.submitted);
  });
});

describe("failClosedSubmit", () => {
  it("the default submit fails closed instead of fabricating an id", async () => {
    await expect(failClosedSubmit({ draft: emptyDraft() })).rejects.toThrow(/unavailable/i);
  });
});
