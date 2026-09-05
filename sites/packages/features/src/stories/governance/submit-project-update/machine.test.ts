import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  projectUpdateMachine,
  UPDATE_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveUpdateSnapshot,
  slugToState,
  stateToSlug,
  simulatePublish,
  type PublishFn,
  type PublishResult,
  type TrackFn,
} from "./machine";

const PROJECT_ID = "b783aa8f-ebf2-4792-b3eb-8dfccf369dfb";
const RESULT: PublishResult = { updateId: "stub-update-b783aa8f-abc" };

const okPublish: PublishFn = async () => RESULT;
const failPublish: PublishFn = async () => {
  throw new Error("governance api unreachable");
};

function inputFor(publishUpdate: PublishFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "governance-submit-project-update",
      variant: "wizard",
      experimentKey: "gv_project_update_wizard",
    },
    projectId: PROJECT_ID,
    publishUpdate,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "general",
  "financials",
  "preview",
  "publishing",
  "publishError",
  "success",
]);

const TRAVERSAL_EVENTS = [
  { type: "SET_GENERAL" as const, health: "onTrack" },
  {
    type: "SET_FINANCIALS" as const,
    csv: "category,description,token,amount,receiver,link",
    disclosed: 5000,
    records: 1,
  },
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "PUBLISH" as const },
  { type: "RETRY" as const },
];

describe("projectUpdateMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(projectUpdateMachine.states));
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
      "general",
      "financials",
      "preview",
      "publishing",
      "success",
    ]) {
      expect(EXPECTED_STATES.has(slugToState(step))).toBe(true);
    }
  });

  it("unknown/missing ?step falls back to the first step", () => {
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.general);
    expect(slugToState(null)).toBe("general");
    expect(slugToState(undefined)).toBe("general");
    expect(slugToState("")).toBe("general");
    expect(slugToState("nope")).toBe("general");
    expect(slugToState("financials")).toBe("financials");
    expect(slugToState("publish-error")).toBe("publishError");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("projectUpdateMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveUpdateSnapshot({
      step: "general",
      trackCtx: inputFor(okPublish, () => {}).trackCtx,
      projectId: PROJECT_ID,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating preview does NOT fire the previewed entry telemetry", async () => {
    const track = vi.fn();
    const snapshot = resolveUpdateSnapshot({
      step: "preview",
      trackCtx: inputFor(okPublish, track).trackCtx,
      projectId: PROJECT_ID,
      track,
    });
    const actor = createActor(projectUpdateMachine, {
      input: inputFor(okPublish, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("preview")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
  });

  it("hydrating publishing does NOT fire telemetry and does NOT auto-publish", async () => {
    const track = vi.fn();
    const publishUpdate = vi.fn(okPublish);
    const snapshot = resolveUpdateSnapshot({
      step: "publishing",
      trackCtx: inputFor(publishUpdate, track).trackCtx,
      projectId: PROJECT_ID,
      publishUpdate,
      track,
    });
    const actor = createActor(projectUpdateMachine, {
      input: inputFor(publishUpdate, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("publishing")).toBe(true);
    expect(actor.getSnapshot().context.draft.health).toBe("onTrack");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(publishUpdate).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("publishing")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveUpdateSnapshot({
      step: "preview",
      trackCtx: inputFor(okPublish, track).trackCtx,
      projectId: PROJECT_ID,
      track,
    });
    const actor = createActor(projectUpdateMachine, {
      input: inputFor(okPublish, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("preview")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "PUBLISH" });
    expect(actor.getSnapshot().matches("publishing")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(
      UPDATE_EVENTS.publishAttempted,
    );
  });
});

describe("projectUpdateMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(projectUpdateMachine, {
      input: inputFor(okPublish, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("financials")).toBe(true);
    expect(ends.has("preview")).toBe(true);
    expect(ends.has("publishing")).toBe(true);
  });

  it("reaching publishing passes through the full step sequence", () => {
    const paths = getShortestPaths(projectUpdateMachine, {
      input: inputFor(okPublish, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const publishing = paths.find(
      (p) => (p.state.value as string) === "publishing",
    );
    expect(publishing).toBeDefined();
    const events = publishing!.steps.map((s) => s.event.type);
    expect(events).toContain("NEXT");
    expect(events).toContain("PUBLISH");
  });
});

describe("projectUpdateMachine \u{2014} telemetry events (happy path)", () => {
  it("full flow fires the complete funnel in order", async () => {
    const track = vi.fn();
    const actor = createActor(projectUpdateMachine, {
      input: inputFor(okPublish, track),
    }).start();

    actor.send({
      type: "SET_GENERAL",
      health: "atRisk",
      fields: {
        introduction: "Intro",
        highlights: "Highlights",
        blockers: "Blockers",
        next_steps: "Next",
      },
    });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("financials")).toBe(true);

    actor.send({
      type: "SET_FINANCIALS",
      csv: "category,description,token,amount,receiver,link",
      disclosed: 5000,
      records: 1,
    });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("preview")).toBe(true);

    actor.send({ type: "PUBLISH" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(UPDATE_EVENTS.started);
    expect(events).toContain(UPDATE_EVENTS.financialsSet);
    expect(events).toContain(UPDATE_EVENTS.previewed);
    expect(events).toContain(UPDATE_EVENTS.publishAttempted);
    expect(events).toContain(UPDATE_EVENTS.published);

    const idx = (e: string) => events.indexOf(e);
    expect(idx(UPDATE_EVENTS.started)).toBeLessThan(idx(UPDATE_EVENTS.financialsSet));
    expect(idx(UPDATE_EVENTS.financialsSet)).toBeLessThan(idx(UPDATE_EVENTS.previewed));
    expect(idx(UPDATE_EVENTS.previewed)).toBeLessThan(idx(UPDATE_EVENTS.publishAttempted));
    expect(idx(UPDATE_EVENTS.publishAttempted)).toBeLessThan(idx(UPDATE_EVENTS.published));

    const startedCall = track.mock.calls.find((c) => c[0] === UPDATE_EVENTS.started);
    expect(startedCall?.[1]).toMatchObject({ health: "atRisk", project_id: PROJECT_ID });
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "gv_project_update_wizard",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);

    const publishedCall = track.mock.calls.find((c) => c[0] === UPDATE_EVENTS.published);
    expect(publishedCall?.[1]).toMatchObject({
      simulated: true,
      project_id: PROJECT_ID,
      update_id: RESULT.updateId,
    });
  });

  it("BACK steps return without re-firing forward telemetry", () => {
    const track = vi.fn();
    const actor = createActor(projectUpdateMachine, {
      input: inputFor(okPublish, track),
    }).start();

    actor.send({ type: "SET_GENERAL", health: "onTrack" });
    actor.send({ type: "NEXT" });
    actor.send({
      type: "SET_FINANCIALS",
      csv: "x",
      disclosed: 0,
      records: 0,
    });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("preview")).toBe(true);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("financials")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("general")).toBe(true);

    const started = track.mock.calls.filter((c) => c[0] === UPDATE_EVENTS.started);
    expect(started.length).toBe(1);
    const previewed = track.mock.calls.filter((c) => c[0] === UPDATE_EVENTS.previewed);
    expect(previewed.length).toBe(1);
  });
});

describe("projectUpdateMachine \u{2014} publish failure + retry", () => {
  it("publish error -> RETRY recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const publishUpdate: PublishFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("governance api unreachable");
      return okPublish(args);
    };

    const actor = createActor(projectUpdateMachine, {
      input: inputFor(publishUpdate, track),
    }).start();

    actor.send({ type: "SET_GENERAL", health: "onTrack" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SET_FINANCIALS", csv: "x", disclosed: 0, records: 0 });
    actor.send({ type: "NEXT" });
    actor.send({ type: "PUBLISH" });
    await waitFor(actor, (s) => s.matches("publishError"));
    expect(actor.getSnapshot().context.error).toBe("governance api unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(UPDATE_EVENTS.published);
  });

  it("publish error -> BACK returns to preview without publishing", async () => {
    const track = vi.fn();
    const actor = createActor(projectUpdateMachine, {
      input: inputFor(failPublish, track),
    }).start();

    actor.send({ type: "SET_GENERAL", health: "onTrack" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "SET_FINANCIALS", csv: "x", disclosed: 0, records: 0 });
    actor.send({ type: "NEXT" });
    actor.send({ type: "PUBLISH" });
    await waitFor(actor, (s) => s.matches("publishError"));

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("preview")).toBe(true);
  });
});

describe("simulatePublish", () => {
  it("resolves a stub update id keyed by project (no network)", async () => {
    const r = await simulatePublish({ projectId: PROJECT_ID, health: "onTrack" });
    expect(r.updateId).toContain("stub-update-b783aa8f");
  });
});
