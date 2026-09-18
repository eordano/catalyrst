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

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("poiMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to coordinates", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(poiMachine.states)));
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.coordinates);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("coordinates");
    }
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("poiMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots coordinates without a snapshot, hydrates later steps silently, and only real transitions track", async () => {
    const track = vi.fn();
    const submit = vi.fn(okSubmit);
    const trackCtx = inputFor(submit, track).trackCtx;
    expect(resolvePoiSnapshot({ step: "coordinates", trackCtx, request: "add" })).toBeUndefined();

    const submitting = createActor(poiMachine, {
      input: inputFor(submit, track),
      snapshot: resolvePoiSnapshot({
        step: "submitting",
        trackCtx,
        request: "add",
        submit,
        track,
        draft: VALID_DRAFT,
      }),
    }).start();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);
    expect(submitting.getSnapshot().context.request).toBe("add");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(submit).not.toHaveBeenCalled();
    expect(submitting.getSnapshot().matches("submitting")).toBe(true);

    const review = createActor(poiMachine, {
      input: inputFor(okSubmit, track),
      snapshot: resolvePoiSnapshot({
        step: "review",
        trackCtx,
        request: "add",
        track,
        draft: VALID_DRAFT,
      }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    review.send({ type: "CONFIRM" });
    expect(review.getSnapshot().matches("submitting")).toBe(true);
    expect(names(track)).toContain(POI_EVENTS.submitting);
  });
});

describe("poiMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("the funnel states are event-reachable and review needs coordinates + description", () => {
    const paths = getShortestPaths(poiMachine, {
      input: inputFor(okSubmit, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) ends.add(p.state.value as string);
    for (const s of ["coordinates", "description", "review", "submitting"]) {
      expect(ends.has(s)).toBe(true);
    }
    const review = paths.find((p) => (p.state.value as string) === "review");
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("SUBMIT_COORDINATES");
    expect(events).toContain("SUBMIT_DESCRIPTION");
  });
});

describe("poiMachine \u{2014} happy path", () => {
  it("accumulates the draft across steps and fires the full funnel through success", async () => {
    const track = vi.fn();
    const actor = createActor(poiMachine, {
      input: inputFor(okSubmit, track),
    }).start();
    const co = ["0x" + "a".repeat(40)];

    expect(names(track)).toContain(POI_EVENTS.started);
    actor.send({ type: "SUBMIT_COORDINATES", x: VALID_DRAFT.x, y: VALID_DRAFT.y });
    expect(actor.getSnapshot().matches("description")).toBe(true);
    actor.send({ type: "SUBMIT_DESCRIPTION", description: VALID_DRAFT.description, coAuthors: co });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(actor.getSnapshot().context.draft).toMatchObject({ ...VALID_DRAFT, coAuthors: co });

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    for (const e of [
      POI_EVENTS.started,
      POI_EVENTS.coordinatesSubmitted,
      POI_EVENTS.descriptionSubmitted,
      POI_EVENTS.reviewReached,
      POI_EVENTS.submitting,
      POI_EVENTS.submitted,
    ]) {
      expect(events).toContain(e);
    }
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
});

describe("poiMachine \u{2014} inline validation (guardrail)", () => {
  it("invalid coordinates and a too-short description stay on their step with inline errors", () => {
    const track = vi.fn();
    const actor = createActor(poiMachine, {
      input: inputFor(okSubmit, track),
    }).start();

    actor.send({ type: "SUBMIT_COORDINATES", x: "9999", y: "0" });
    expect(actor.getSnapshot().matches("coordinates")).toBe(true);
    expect(actor.getSnapshot().context.errors.x).toBeTruthy();
    expect(names(track)).toContain(POI_EVENTS.coordinatesInvalid);
    expect(names(track)).not.toContain(POI_EVENTS.coordinatesSubmitted);

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
    actor.send({ type: "SUBMIT_DESCRIPTION", description: VALID_DRAFT.description, coAuthors: [] });
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("createProposal unavailable");
    expect(names(track)).toContain(POI_EVENTS.error);

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(names(track)).toContain(POI_EVENTS.submitted);
  });
});

describe("simulateSubmit", () => {
  it("resolves a synthetic stub proposal id (no network)", async () => {
    const out = await simulateSubmit({ request: "add", draft: VALID_DRAFT });
    expect(out.stub).toBe(true);
    expect(out.proposalId).toBeTruthy();
  });
});
