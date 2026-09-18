import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  worldSettingsMachine,
  WORLD_SETTINGS_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveWorldSettingsSnapshot,
  slugToState,
  stateToSlug,
  tabToUi3,
  simulateSave,
  type SaveFn,
  type TrackFn,
} from "./machine";

const okSave: SaveFn = async ({ worldName, changes }) => ({
  worldName,
  savedFields: Object.keys(changes),
});

function inputFor(save: SaveFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "creator-hub-world-settings",
      variant: "wizard",
      experimentKey: "ch_world_settings_wizard",
    },
    worldName: "neon-market.dcl.eth",
    save,
    track,
  };
}

const TRAVERSAL_EVENTS = [
  { type: "NEXT" as const },
  { type: "BACK" as const },
  { type: "REVIEW" as const },
  { type: "DISCARD" as const },
  { type: "SAVE" as const },
  { type: "RETRY" as const },
  { type: "GO_TAB" as const, tab: "details" as const },
  { type: "GO_TAB" as const, tab: "layout" as const },
  { type: "GO_TAB" as const, tab: "misc" as const },
  { type: "CHANGE" as const, tab: "details" as const, field: "title" },
];

async function savedEventProps(save: SaveFn) {
  const track = vi.fn();
  const actor = createActor(worldSettingsMachine, {
    input: inputFor(save, track),
  }).start();

  actor.send({ type: "CHANGE", tab: "details", field: "title" });
  actor.send({ type: "NEXT" });
  actor.send({ type: "NEXT" });
  actor.send({ type: "REVIEW" });
  actor.send({ type: "SAVE" });
  await waitFor(actor, (s) => s.matches("saved"));

  const call = track.mock.calls.find((c) => c[0] === WORLD_SETTINGS_EVENTS.saved);
  expect(call).toBeDefined();
  return call![1] as Record<string, unknown>;
}

describe("worldSettingsMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, falls back to the first step, and maps the misc tab to the ui3 general prop", () => {
    const machineStates = new Set(Object.keys(worldSettingsMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.details);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("details");
    }
    for (const same of ["misc", "review", "saving"]) {
      expect(slugToState(same)).toBe(same);
    }
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);

    expect(tabToUi3("details")).toBe("details");
    expect(tabToUi3("layout")).toBe("layout");
    expect(tabToUi3("misc")).toBe("general");
  });
});

describe("worldSettingsMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot; saving hydrates without telemetry or auto-save; a real transition after hydration fires", async () => {
    const trackCtx = inputFor(okSave, () => {}).trackCtx;
    expect(resolveWorldSettingsSnapshot({ step: "details", trackCtx })).toBeUndefined();

    const track = vi.fn();
    const save = vi.fn(okSave);
    const saving = createActor(worldSettingsMachine, {
      input: inputFor(save, track),
      snapshot: resolveWorldSettingsSnapshot({ step: "saving", trackCtx, save, track }),
    }).start();
    expect(saving.getSnapshot().matches("saving")).toBe(true);
    expect(Object.keys(saving.getSnapshot().context.changes).length).toBeGreaterThan(0);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
    expect(saving.getSnapshot().matches("saving")).toBe(true);

    const review = createActor(worldSettingsMachine, {
      input: inputFor(okSave, track),
      snapshot: resolveWorldSettingsSnapshot({ step: "review", trackCtx, track }),
    }).start();
    expect(review.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    review.send({ type: "DISCARD" });
    expect(review.getSnapshot().matches("details")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(WORLD_SETTINGS_EVENTS.discarded);
    expect(Object.keys(review.getSnapshot().context.changes)).toHaveLength(0);
  });
});

describe("worldSettingsMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("the funnel states are event-reachable and review needs REVIEW", () => {
    const paths = getShortestPaths(worldSettingsMachine, {
      input: inputFor(okSave, () => {}),
      events: TRAVERSAL_EVENTS,
    });

    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) ends.add(p.state.value as string);
    for (const s of ["details", "layout", "misc", "review", "saving"]) {
      expect(ends.has(s)).toBe(true);
    }

    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    expect(review!.steps.map((s) => s.event.type)).toContain("REVIEW");
  });
});

describe("worldSettingsMachine \u{2014} telemetry events (happy path)", () => {
  it("details -> layout -> misc -> review -> save -> saved fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(worldSettingsMachine, {
      input: inputFor(okSave, track),
    }).start();

    let events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(WORLD_SETTINGS_EVENTS.opened);
    expect(events).toContain(WORLD_SETTINGS_EVENTS.tabViewed);

    actor.send({ type: "CHANGE", tab: "details", field: "title" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("layout")).toBe(true);
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("misc")).toBe(true);
    actor.send({ type: "CHANGE", tab: "misc", field: "showInPlaces" });
    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("saved"));

    events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        WORLD_SETTINGS_EVENTS.changed,
        WORLD_SETTINGS_EVENTS.reviewReached,
        WORLD_SETTINGS_EVENTS.saving,
        WORLD_SETTINGS_EVENTS.saved,
      ]),
    );
    expect(events.indexOf(WORLD_SETTINGS_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(WORLD_SETTINGS_EVENTS.saving),
    );
    expect(events.indexOf(WORLD_SETTINGS_EVENTS.saving)).toBeLessThan(
      events.indexOf(WORLD_SETTINGS_EVENTS.saved),
    );

    expect(actor.getSnapshot().context.result?.savedFields).toEqual(
      expect.arrayContaining(["details.title", "misc.showInPlaces"]),
    );
    const openedCall = track.mock.calls.find((c) => c[0] === WORLD_SETTINGS_EVENTS.opened);
    expect(openedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "ch_world_settings_wizard",
      variant: "wizard",
    });
  });

  it("GO_TAB jumps directly (tab_viewed), leaving review via GO_TAB keeps changes, and DISCARD from review drops them without saving", () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    const actor = createActor(worldSettingsMachine, {
      input: inputFor(save, track),
    }).start();

    actor.send({ type: "CHANGE", tab: "details", field: "title" });
    actor.send({ type: "GO_TAB", tab: "misc" });
    expect(actor.getSnapshot().matches("misc")).toBe(true);
    const tabViews = track.mock.calls
      .filter((c) => c[0] === WORLD_SETTINGS_EVENTS.tabViewed)
      .map((c) => (c[1] as { tab: string }).tab);
    expect(tabViews).toContain("misc");

    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("review")).toBe(true);
    actor.send({ type: "GO_TAB", tab: "layout" });
    expect(actor.getSnapshot().matches("layout")).toBe(true);
    expect(Object.keys(actor.getSnapshot().context.changes)).toEqual(["details.title"]);
    expect(track.mock.calls.filter((c) => c[0] === WORLD_SETTINGS_EVENTS.discarded)).toHaveLength(0);

    actor.send({ type: "NEXT" });
    actor.send({ type: "REVIEW" });
    actor.send({ type: "DISCARD" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(WORLD_SETTINGS_EVENTS.discarded);
    expect(events).not.toContain(WORLD_SETTINGS_EVENTS.saved);
    expect(save).not.toHaveBeenCalled();
    expect(Object.keys(actor.getSnapshot().context.changes)).toHaveLength(0);
  });
});

describe("worldSettingsMachine \u{2014} save failure + retry", () => {
  it("save error -> RETRY recovers to saved", async () => {
    const track = vi.fn();
    let calls = 0;
    const save: SaveFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("worlds-content-server unreachable");
      return okSave(args);
    };

    const actor = createActor(worldSettingsMachine, {
      input: inputFor(save, track),
    }).start();

    actor.send({ type: "CHANGE", tab: "details", field: "title" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "NEXT" });
    actor.send({ type: "REVIEW" });
    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("worlds-content-server unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("saved"));
    expect(track.mock.calls.map((c) => c[0])).toContain(WORLD_SETTINGS_EVENTS.saved);
  });
});

describe("worldSettingsMachine \u{2014} stub telemetry contract", () => {
  it("simulateSave lists the saved fields and marks stub:true; the saved event echoes stub:true for the stub and stub:false for a real (or stub-less) SaveFn", async () => {
    const res = await simulateSave({
      worldName: "neon-market.dcl.eth",
      changes: { "details.title": true, "misc.singlePlayer": true },
    });
    expect(res.worldName).toBe("neon-market.dcl.eth");
    expect(res.savedFields).toEqual(
      expect.arrayContaining(["details.title", "misc.singlePlayer"]),
    );
    expect(res.stub).toBe(true);

    expect(await savedEventProps(simulateSave)).toMatchObject({ stub: true });

    const realSave: SaveFn = async ({ worldName, changes }) => ({
      worldName,
      savedFields: Object.keys(changes),
      stub: false,
    });
    const realProps = await savedEventProps(realSave);
    expect(realProps).toMatchObject({ stub: false });
    expect(realProps.stub).not.toBe(true);

    const impliedRealSave: SaveFn = async ({ worldName, changes }) => ({
      worldName,
      savedFields: Object.keys(changes),
    });
    expect(await savedEventProps(impliedRealSave)).toMatchObject({ stub: false });
  });
});
