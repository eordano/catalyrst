import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  poiMachine,
  POI_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolvePoiSnapshot,
  slugToState,
  stateToSlug,
  simulateSubmit,
  type SubmitFn,
  type SubmitResult,
  type TrackFn,
  type PoiDraft,
} from "./machine";

const RESULT: SubmitResult = { proposalId: "sim-poi-test", stub: true };

const okSubmit: SubmitFn = async () => RESULT;

const VALID_DRAFT: PoiDraft = {
  x: "12",
  y: "42",
  description: "This scene is a stunning interactive art gallery worth pinning.",
  coAuthors: [],
};

function inputFor(submit: SubmitFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-poi",
      variant: "wizard",
      experimentKey: "gv_poi_wizard",
    },
    request: "add" as const,
    submit,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "coordinates",
  "description",
  "review",
  "submitting",
  "success",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "SUBMIT_COORDINATES" as const, x: "12", y: "42" },
  { type: "SUBMIT_COORDINATES" as const, x: "9999", y: "0" },
  {
    type: "SUBMIT_DESCRIPTION" as const,
    description: VALID_DRAFT.description,
    coAuthors: [] as string[],
  },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("poiMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(poiMachine.states));
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

  it("the audit-spec step slugs are all addressable", () => {
    for (const slug of ["coordinates", "description", "review", "submitting", "success"]) {
      expect(SLUG_TO_STATE[slug as keyof typeof SLUG_TO_STATE]).toBeDefined();
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.coordinates);
    expect(slugToState(null)).toBe("coordinates");
    expect(slugToState(undefined)).toBe("coordinates");
    expect(slugToState("")).toBe("coordinates");
    expect(slugToState("nope")).toBe("coordinates");
    expect(slugToState("description")).toBe("description");
    expect(slugToState("review")).toBe("review");
    expect(slugToState("submitting")).toBe("submitting");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("poiMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolvePoiSnapshot({
      step: "coordinates",
      trackCtx: inputFor(okSubmit, () => {}).trackCtx,
      request: "add",
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step fires NO telemetry and does NOT auto-submit", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const snapshot = resolvePoiSnapshot({
      step: "submitting",
      trackCtx: inputFor(submit, track).trackCtx,
      request: "add",
      submit,
      track,
      draft: VALID_DRAFT,
    });
    const actor = createActor(poiMachine, {
      input: inputFor(submit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(actor.getSnapshot().context.request).toBe("add");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolvePoiSnapshot({
      step: "review",
      trackCtx: inputFor(okSubmit, track).trackCtx,
      request: "add",
      track,
      draft: VALID_DRAFT,
    });
    const actor = createActor(poiMachine, {
      input: inputFor(okSubmit, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("submitting")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(POI_EVENTS.submitting);
  });
});

describe("poiMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(poiMachine, {
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
    expect(ends.has("coordinates")).toBe(true);
    expect(ends.has("description")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("submitting")).toBe(true);
  });

  it("reaching review passes through coordinates + description", () => {
    const paths = getShortestPaths(poiMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("SUBMIT_COORDINATES");
    expect(events).toContain("SUBMIT_DESCRIPTION");
  });
});

describe("poiMachine \u{2014} telemetry events (happy path)", () => {
  it("coords -> description -> review -> confirm -> submit fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(poiMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    expect(track.mock.calls.map((c) => c[0])).toContain(POI_EVENTS.started);

    actor.send({ type: "SUBMIT_COORDINATES", x: VALID_DRAFT.x, y: VALID_DRAFT.y });
    expect(actor.getSnapshot().matches("description")).toBe(true);

    actor.send({
      type: "SUBMIT_DESCRIPTION",
      description: VALID_DRAFT.description,
      coAuthors: [],
    });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(POI_EVENTS.started);
    expect(events).toContain(POI_EVENTS.coordinatesSubmitted);
    expect(events).toContain(POI_EVENTS.descriptionSubmitted);
    expect(events).toContain(POI_EVENTS.reviewReached);
    expect(events).toContain(POI_EVENTS.submitting);
    expect(events).toContain(POI_EVENTS.submitted);

    expect(events.indexOf(POI_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(POI_EVENTS.submitted),
    );

    const startedCall = track.mock.calls.find((c) => c[0] === POI_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_poi_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("the draft is accumulated across steps", () => {
    const actor = createActor(poiMachine, {
      input: inputFor(okSubmit, vi.fn()),
    }).start();
    actor.send({ type: "SUBMIT_COORDINATES", x: "12", y: "42" });
    actor.send({
      type: "SUBMIT_DESCRIPTION",
      description: VALID_DRAFT.description,
      coAuthors: ["0x" + "a".repeat(40)],
    });
    const draft = actor.getSnapshot().context.draft;
    expect(draft.x).toBe("12");
    expect(draft.y).toBe("42");
    expect(draft.description).toBe(VALID_DRAFT.description);
    expect(draft.coAuthors).toHaveLength(1);
  });
});

describe("poiMachine \u{2014} inline validation (guardrail)", () => {
  it("invalid coordinates stay on the step and emit gv_poi_coordinates_invalid", () => {
    const track = vi.fn();
    const actor = createActor(poiMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "SUBMIT_COORDINATES", x: "9999", y: "0" });
    expect(actor.getSnapshot().matches("coordinates")).toBe(true);
    expect(actor.getSnapshot().context.errors.x).toBeTruthy();

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(POI_EVENTS.coordinatesInvalid);
    expect(events).not.toContain(POI_EVENTS.coordinatesSubmitted);
  });

  it("too-short description stays on the step and surfaces an inline error", () => {
    const actor = createActor(poiMachine, {
      input: inputFor(okSubmit, vi.fn()),
    }).start();
    actor.send({ type: "SUBMIT_COORDINATES", x: "12", y: "42" });
    actor.send({ type: "SUBMIT_DESCRIPTION", description: "too short", coAuthors: [] });
    expect(actor.getSnapshot().matches("description")).toBe(true);
    expect(actor.getSnapshot().context.errors.description).toBeTruthy();
  });
});

describe("poiMachine \u{2014} submit failure + retry", () => {
  it("submit error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const submit: SubmitFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("createProposal unavailable");
      return okSubmit(args);
    };

    const actor = createActor(poiMachine, {
      input: inputFor(submit, track),
    }).start();

    actor.send({ type: "SUBMIT_COORDINATES", x: "12", y: "42" });
    actor.send({
      type: "SUBMIT_DESCRIPTION",
      description: VALID_DRAFT.description,
      coAuthors: [],
    });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("createProposal unavailable");
    expect(track.mock.calls.map((c) => c[0])).toContain(POI_EVENTS.error);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(track.mock.calls.map((c) => c[0])).toContain(POI_EVENTS.submitted);
  });
});

describe("simulateSubmit", () => {
  it("resolves a synthetic stub proposal id (no network)", async () => {
    const out = await simulateSubmit({ request: "add", draft: VALID_DRAFT });
    expect(out.stub).toBe(true);
    expect(out.proposalId).toContain("sim-poi-");
  });
});
