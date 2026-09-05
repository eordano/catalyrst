import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  submitCatalystMachine,
  CATALYST_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveCatalystSnapshot,
  slugToState,
  stateToSlug,
  defaultCreate,
  type CreateFn,
  type TrackFn,
} from "./machine";
import type { CreatedProposal } from "@data/lib/catalyst/governance/submit-catalyst";

const RESULT: CreatedProposal = {
  id: "00000000-0000-0000-0000-000000000abc",
  type: "catalyst_add",
  request: "add",
};

const okCreate: CreateFn = async () => RESULT;
const failCreate: CreateFn = async () => {
  throw new Error("governance unreachable");
};

function inputFor(create: CreateFn, track: TrackFn, request: "add" | "remove" = "add") {
  return {
    request,
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-catalyst",
      variant: "wizard",
      experimentKey: "gv_catalyst_wizard",
    },
    create,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "details",
  "description",
  "review",
  "submitting",
  "success",
  "error",
]);

describe("submitCatalystMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(submitCatalystMachine.states));
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
    expect(slugToState("description")).toBe("description");
    expect(slugToState("review")).toBe("review");
    expect(slugToState("submitting")).toBe("submitting");
    expect(slugToState("success")).toBe("success");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("submitCatalystMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveCatalystSnapshot({
      step: "details",
      request: "add",
      trackCtx: inputFor(okCreate, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const create = vi.fn(okCreate);
    const snapshot = resolveCatalystSnapshot({
      step: "submitting",
      request: "remove",
      trackCtx: inputFor(create, track, "remove").trackCtx,
      create,
      track,
    });
    const actor = createActor(submitCatalystMachine, {
      input: inputFor(create, track, "remove"),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.request).toBe("remove");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("hydrating review does NOT fire trackReviewReached (no entry-action replay)", () => {
    const track = vi.fn();
    const snapshot = resolveCatalystSnapshot({
      step: "review",
      request: "add",
      trackCtx: inputFor(okCreate, track).trackCtx,
      track,
    });
    const actor = createActor(submitCatalystMachine, {
      input: inputFor(okCreate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveCatalystSnapshot({
      step: "description",
      request: "add",
      trackCtx: inputFor(okCreate, track).trackCtx,
      track,
    });
    const actor = createActor(submitCatalystMachine, {
      input: inputFor(okCreate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("description")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "FILL_DESCRIPTION", description: "x".repeat(40), coAuthors: [] });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CATALYST_EVENTS.descriptionFilled);
    expect(events).toContain(CATALYST_EVENTS.reviewReached);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "FILL_DETAILS" as const, owner: "0x06012c8cf97bead5deae237070f9587f8e7a266d", domain: "peer.example.com" },
  { type: "DOMAIN_INVALID" as const },
  { type: "FILL_DESCRIPTION" as const, description: "x".repeat(40), coAuthors: [] },
  { type: "SUBMIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("submitCatalystMachine \u{2014} model-based path coverage", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(submitCatalystMachine, {
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
    expect(ends.has("description")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching review passes through FILL_DETAILS and FILL_DESCRIPTION", () => {
    const paths = getShortestPaths(submitCatalystMachine, {
      input: inputFor(okCreate, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("FILL_DETAILS");
    expect(events).toContain("FILL_DESCRIPTION");
  });
});

describe("submitCatalystMachine \u{2014} telemetry (happy path)", () => {
  it("details -> description -> review -> submit -> success fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(submitCatalystMachine, {
      input: inputFor(okCreate, track),
    }).start();

    actor.send({
      type: "FILL_DETAILS",
      owner: "0x06012c8cf97bead5deae237070f9587f8e7a266d",
      domain: "peer.example.com",
      alreadyACatalyst: false,
    });
    expect(actor.getSnapshot().matches("description")).toBe(true);

    actor.send({ type: "FILL_DESCRIPTION", description: "x".repeat(40), coAuthors: ["0x" + "1".repeat(40)] });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CATALYST_EVENTS.started);
    expect(events).toContain(CATALYST_EVENTS.detailsFilled);
    expect(events).toContain(CATALYST_EVENTS.descriptionFilled);
    expect(events).toContain(CATALYST_EVENTS.reviewReached);
    expect(events).toContain(CATALYST_EVENTS.submitting);
    expect(events).toContain(CATALYST_EVENTS.submitted);

    expect(events.indexOf(CATALYST_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(CATALYST_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === CATALYST_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_catalyst_wizard",
      variant: "wizard",
    });
    const submittedCall = track.mock.calls.find((c) => c[0] === CATALYST_EVENTS.submitted);
    expect(submittedCall?.[1]).toMatchObject({ request: "add" });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("the invalid-domain guardrail event fires without advancing", () => {
    const track = vi.fn();
    const actor = createActor(submitCatalystMachine, {
      input: inputFor(okCreate, track),
    }).start();

    actor.send({ type: "DOMAIN_INVALID" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(CATALYST_EVENTS.domainInvalid);
    expect(events).not.toContain(CATALYST_EVENTS.started);
  });
});

describe("submitCatalystMachine \u{2014} submit failure + retry", () => {
  it("submit error fires gv_catalyst_submit_error and RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const create: CreateFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance unreachable");
      return okCreate(args);
    };

    const actor = createActor(submitCatalystMachine, {
      input: inputFor(create, track, "remove"),
    }).start();

    actor.send({
      type: "FILL_DETAILS",
      owner: "0x06012c8cf97bead5deae237070f9587f8e7a266d",
      domain: "peer.example.com",
    });
    actor.send({ type: "FILL_DESCRIPTION", description: "x".repeat(40) });
    actor.send({ type: "SUBMIT" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("governance unreachable");
    expect(track.mock.calls.map((c) => c[0])).toContain(CATALYST_EVENTS.submitError);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(CATALYST_EVENTS.submitted);
  });
});

describe("defaultCreate", () => {
  it("fails closed instead of fabricating a proposal id", async () => {
    await expect(
      defaultCreate({
        request: "add",
        details: { owner: "0x06012c8cf97bead5deae237070f9587f8e7a266d", domain: "peer.example.com", alreadyACatalyst: false },
        rationale: { description: "x".repeat(40), coAuthors: [] },
      }),
    ).rejects.toThrow(
      "catalyst proposal submission unavailable: DAO governance signer not configured",
    );
  });
});
