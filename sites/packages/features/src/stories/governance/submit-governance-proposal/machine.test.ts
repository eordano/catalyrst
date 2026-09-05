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
const failSubmit: SubmitFn = async () => {
  throw new Error("governance api unreachable");
};

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

const EXPECTED_STATES = new Set([
  "intro",
  "details",
  "coauthors",
  "review",
  "submitting",
  "submitError",
  "success",
]);

describe("govProposalMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(govProposalMachine.states));
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
    expect(slugToState("review")).toBe("review");
    expect(slugToState("submit-error")).toBe("submitError");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("govProposalMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveGovProposalSnapshot({
      step: "intro",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
      votingPower: 3000,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const snapshot = resolveGovProposalSnapshot({
      step: "submitting",
      trackCtx: inputFor(submit, track).trackCtx,
      votingPower: 3000,
      submit,
      track,
    });
    const actor = createActor(govProposalMachine, {
      input: inputFor(submit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.draft.title).toContain("Formalize");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveGovProposalSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      votingPower: 3000,
      track,
    });
    const actor = createActor(govProposalMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(
      GOVPROP_EVENTS.submitAttempted,
    );
  });
});

const TRAVERSAL_EVENTS = [
  { type: "START" as const },
  validDetails(),
  { type: "SET_COAUTHORS" as const, coAuthors: [] },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

describe("govProposalMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(govProposalMachine, {
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
    const paths = getShortestPaths(govProposalMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("START");
    expect(events).toContain("SUBMIT_DETAILS");
    expect(events).toContain("NEXT");
    expect(events).toContain("SUBMIT");
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
    expect(events).toContain(GOVPROP_EVENTS.started);
    expect(events).toContain(GOVPROP_EVENTS.detailsSubmitted);
    expect(events).toContain(GOVPROP_EVENTS.stepAdvanced);
    expect(events).toContain(GOVPROP_EVENTS.submitAttempted);
    expect(events).toContain(GOVPROP_EVENTS.submitted);

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
  it("START under the threshold stays on intro and logs the guardrail", () => {
    const track = vi.fn();
    const actor = createActor(govProposalMachine, {
      input: inputFor(okSubmit, track, 1000),
    }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("intro")).toBe(true);

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(GOVPROP_EVENTS.vpBlocked);
    expect(events).not.toContain(GOVPROP_EVENTS.started);
  });

  it("START at exactly the threshold advances", () => {
    const track = vi.fn();
    const actor = createActor(govProposalMachine, {
      input: inputFor(okSubmit, track, GOVERNANCE_SCHEMA.vpThreshold),
    }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(GOVPROP_EVENTS.started);
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
  it("submit error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submit: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance api unreachable");
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

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(GOVPROP_EVENTS.error);
    expect(events).toContain(GOVPROP_EVENTS.submitted);
  });

  it("submit error -> BACK returns to review without submitting", async () => {
    const track = vi.fn();
    const actor = createActor(govProposalMachine, {
      input: inputFor(failSubmit, track),
    }).start();

    actor.send({ type: "START" });
    actor.send(validDetails());
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });
});

describe("failClosedSubmit + emptyDraft", () => {
  it("emptyDraft has the expected shape", () => {
    expect(emptyDraft()).toEqual({
      linkedDraftId: "",
      title: "",
      bodies: {},
      coAuthors: [],
    });
  });

  it("fails closed instead of fabricating a proposal id", async () => {
    await expect(failClosedSubmit({ draft: emptyDraft() })).rejects.toThrow(
      "governance proposal submission unavailable: DAO governance signer not configured",
    );
  });
});
