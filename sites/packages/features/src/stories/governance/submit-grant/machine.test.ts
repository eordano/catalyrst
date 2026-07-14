import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  grantMachine,
  GRANT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveGrantSnapshot,
  slugToState,
  stateToSlug,
  simulateSubmit,
  type SubmitFn,
  type SubmitResult,
  type TrackFn,
} from "./machine";

const RESULT: SubmitResult = { proposalId: "stub-grant-platform-abc" };

const okSubmit: SubmitFn = async () => RESULT;
const failSubmit: SubmitFn = async () => {
  throw new Error("governance api unreachable");
};

function inputFor(submitGrant: SubmitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-grant",
      variant: "wizard",
      experimentKey: "gv_grant_wizard",
    },
    submitGrant,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "category",
  "funding",
  "general",
  "assessment",
  "review",
  "submitting",
  "submitError",
  "success",
]);

const TRAVERSAL_EVENTS = [
  { type: "PICK_CATEGORY" as const, category: "Platform" },
  { type: "SET_FUNDING" as const, budget: 24000, duration: 6, tier: "Tier 4" },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "SUBMIT" as const },
  { type: "RETRY" as const },
];

describe("grantMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(grantMachine.states));
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
      "category",
      "funding",
      "general",
      "assessment",
      "review",
      "submitting",
      "success",
    ]) {
      expect(EXPECTED_STATES.has(slugToState(step))).toBe(true);
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.category);
    expect(slugToState(null)).toBe("category");
    expect(slugToState(undefined)).toBe("category");
    expect(slugToState("")).toBe("category");
    expect(slugToState("nope")).toBe("category");
    expect(slugToState("funding")).toBe("funding");
    expect(slugToState("submit-error")).toBe("submitError");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("grantMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveGrantSnapshot({
      step: "category",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating submitting does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submitGrant = vi.fn(okSubmit);
    const snapshot = resolveGrantSnapshot({
      step: "submitting",
      trackCtx: inputFor(submitGrant, track).trackCtx,
      submitGrant,
      track,
    });
    const actor = createActor(grantMachine, {
      input: inputFor(submitGrant, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.draft.category).toBe("Platform");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submitGrant).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveGrantSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      track,
    });
    const actor = createActor(grantMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SUBMIT" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(
      GRANT_EVENTS.submitAttempted,
    );
  });
});

describe("grantMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(grantMachine, {
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
    expect(ends.has("funding")).toBe(true);
    expect(ends.has("general")).toBe(true);
    expect(ends.has("assessment")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching submitting passes through the full step sequence", () => {
    const paths = getShortestPaths(grantMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const submitting = paths.find((p) => (p.state.value as string) === "submitting");
    expect(submitting).toBeDefined();
    const events = submitting!.steps.map((s) => s.event.type);
    expect(events).toContain("PICK_CATEGORY");
    expect(events).toContain("SET_FUNDING");
    expect(events).toContain("NEXT");
    expect(events).toContain("SUBMIT");
  });
});

describe("grantMachine \u{2014} telemetry events (happy path)", () => {
  it("full flow fires the complete funnel in order", async () => {
    const track = vi.fn();
    const actor = createActor(grantMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "PICK_CATEGORY", category: "Platform" });
    expect(actor.getSnapshot().matches("funding")).toBe(true);

    actor.send({ type: "SET_FUNDING", budget: 24000, duration: 6, tier: "Tier 4" });
    expect(actor.getSnapshot().matches("general")).toBe(true);

    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(GRANT_EVENTS.started);
    expect(events).toContain(GRANT_EVENTS.fundingSet);
    expect(events).toContain(GRANT_EVENTS.stepAdvanced);
    expect(events).toContain(GRANT_EVENTS.submitAttempted);
    expect(events).toContain(GRANT_EVENTS.submitted);

    expect(events.indexOf(GRANT_EVENTS.submitAttempted)).toBeLessThan(
      events.indexOf(GRANT_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === GRANT_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_grant_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("two step-advanced events fire (general->assessment, assessment->review)", () => {
    const track = vi.fn();
    const actor = createActor(grantMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "PICK_CATEGORY", category: "Core Unit" });
    actor.send({ type: "SET_FUNDING", budget: 5000, duration: 3, tier: "Tier 3" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });

    const advanced = track.mock.calls.filter((c) => c[0] === GRANT_EVENTS.stepAdvanced);
    expect(advanced.length).toBe(2);
    expect(advanced.map((c) => (c[1] as { to: string }).to)).toEqual([
      "assessment",
      "review",
    ]);
  });

  it("BACK steps return without re-firing forward telemetry", () => {
    const track = vi.fn();
    const actor = createActor(grantMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "PICK_CATEGORY", category: "Platform" });
    actor.send({ type: "SET_FUNDING", budget: 1000, duration: 1, tier: "Tier 1" });
    expect(actor.getSnapshot().matches("general")).toBe(true);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("funding")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("category")).toBe(true);

    const started = track.mock.calls.filter((c) => c[0] === GRANT_EVENTS.started);
    expect(started.length).toBe(1);
  });
});

describe("grantMachine \u{2014} submit failure + retry", () => {
  it("submit error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submitGrant: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance api unreachable");
      return okSubmit(args);
    };

    const actor = createActor(grantMachine, {
      input: inputFor(submitGrant, track),
    }).start();

    actor.send({ type: "PICK_CATEGORY", category: "Platform" });
    actor.send({ type: "SET_FUNDING", budget: 24000, duration: 6, tier: "Tier 4" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));
    expect(actor.getSnapshot().context.error).toBe("governance api unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(GRANT_EVENTS.submitted);
  });

  it("submit error -> BACK returns to review without submitting", async () => {
    const track = vi.fn();
    const actor = createActor(grantMachine, {
      input: inputFor(failSubmit, track),
    }).start();

    actor.send({ type: "PICK_CATEGORY", category: "Platform" });
    actor.send({ type: "SET_FUNDING", budget: 24000, duration: 6, tier: "Tier 4" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("submitError"));

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
  });
});

describe("simulateSubmit", () => {
  it("resolves a stub proposal id keyed by category (no network)", async () => {
    const r = await simulateSubmit({ category: "In-World Content", budget: 1000, duration: 2 });
    expect(r.proposalId).toContain("stub-grant-in-world-content");
  });
});
