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

const FINANCIALS = {
  type: "SET_FINANCIALS" as const,
  csv: "category,description,token,amount,receiver,link",
  disclosed: 5000,
  records: 1,
};

const TRAVERSAL_EVENTS = [
  { type: "SET_GENERAL" as const, health: "onTrack" },
  FINANCIALS,
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "PUBLISH" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("projectUpdateMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to general", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(projectUpdateMachine.states)));
    expect(mapped).toEqual(EXPECTED_STATES);
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.general);
    expect(slugToState("publish-error")).toBe("publishError");
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("general");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("projectUpdateMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots general without a snapshot, hydrates preview/publishing silently, and only real transitions track", async () => {
    const track = vi.fn();
    const publishUpdate = vi.fn(okPublish);
    const trackCtx = inputFor(publishUpdate, track).trackCtx;
    expect(
      resolveUpdateSnapshot({ step: "general", trackCtx, projectId: PROJECT_ID }),
    ).toBeUndefined();

    const publishing = createActor(projectUpdateMachine, {
      input: inputFor(publishUpdate, track),
      snapshot: resolveUpdateSnapshot({
        step: "publishing",
        trackCtx,
        projectId: PROJECT_ID,
        publishUpdate,
        track,
      }),
    }).start();
    expect(publishing.getSnapshot().matches("publishing")).toBe(true);
    expect(publishing.getSnapshot().context.draft.health).toBe("onTrack");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(publishUpdate).not.toHaveBeenCalled();
    expect(publishing.getSnapshot().matches("publishing")).toBe(true);

    const preview = createActor(projectUpdateMachine, {
      input: inputFor(okPublish, track),
      snapshot: resolveUpdateSnapshot({ step: "preview", trackCtx, projectId: PROJECT_ID, track }),
    }).start();
    expect(preview.getSnapshot().matches("preview")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    preview.send({ type: "PUBLISH" });
    expect(preview.getSnapshot().matches("publishing")).toBe(true);
    expect(names(track)).toContain(UPDATE_EVENTS.publishAttempted);
  });
});

describe("projectUpdateMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and publishing needs NEXT + PUBLISH", () => {
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
    for (const s of ["financials", "preview", "publishing"]) expect(ends.has(s)).toBe(true);
    const publishing = paths.find((p) => (p.state.value as string) === "publishing");
    const events = publishing!.steps.map((s) => s.event.type);
    expect(events).toContain("NEXT");
    expect(events).toContain("PUBLISH");
  });
});

describe("projectUpdateMachine \u{2014} happy path", () => {
  it("fires the complete funnel in order, and BACK steps never re-fire forward telemetry", async () => {
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
    actor.send(FINANCIALS);
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("preview")).toBe(true);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("financials")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("general")).toBe(true);
    expect(names(track).filter((e) => e === UPDATE_EVENTS.started)).toHaveLength(1);
    expect(names(track).filter((e) => e === UPDATE_EVENTS.previewed)).toHaveLength(1);

    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("preview")).toBe(true);
    actor.send({ type: "PUBLISH" });
    await waitFor(actor, (s) => s.matches("success"));

    const events = names(track);
    const idx = (e: string) => events.indexOf(e);
    for (const e of [
      UPDATE_EVENTS.started,
      UPDATE_EVENTS.financialsSet,
      UPDATE_EVENTS.previewed,
      UPDATE_EVENTS.publishAttempted,
      UPDATE_EVENTS.published,
    ]) {
      expect(events).toContain(e);
    }
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
    const publishedCall = track.mock.calls.find((c) => c[0] === UPDATE_EVENTS.published);
    expect(publishedCall?.[1]).toMatchObject({
      simulated: true,
      project_id: PROJECT_ID,
      update_id: RESULT.updateId,
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });
});

describe("projectUpdateMachine \u{2014} publish failure", () => {
  it("an error offers BACK to preview and RETRY, which recovers to success", async () => {
    const track = vi.fn();
    let calls = 0;
    const publishUpdate: PublishFn = async (args) => {
      calls += 1;
      if (calls < 3) throw new Error("governance api unreachable");
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

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("preview")).toBe(true);
    actor.send({ type: "PUBLISH" });
    await waitFor(actor, (s) => s.matches("publishError"));

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("success"));
    expect(names(track)).toContain(UPDATE_EVENTS.published);
  });
});

describe("simulatePublish", () => {
  it("resolves a stub update id keyed by project (no network)", async () => {
    const r = await simulatePublish({ projectId: PROJECT_ID, health: "onTrack" });
    expect(r.updateId).toContain("stub-update-b783aa8f");
  });
});
