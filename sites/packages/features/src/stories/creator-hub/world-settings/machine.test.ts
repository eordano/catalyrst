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
  type SaveResult,
  type TrackFn,
} from "./machine";

const RESULT: SaveResult = {
  worldName: "neon-market.dcl.eth",
  savedFields: ["details.title"],
};

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

const EXPECTED_STATES = new Set([
  "details",
  "layout",
  "misc",
  "review",
  "saving",
  "saved",
  "error",
]);

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

describe("worldSettingsMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(worldSettingsMachine.states));
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
    expect(slugToState("misc")).toBe("misc");
    expect(slugToState("review")).toBe("review");
    expect(slugToState("saving")).toBe("saving");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });

  it("maps the misc tab to the ui3 `general` prop", () => {
    expect(tabToUi3("details")).toBe("details");
    expect(tabToUi3("layout")).toBe("layout");
    expect(tabToUi3("misc")).toBe("general");
  });
});

describe("worldSettingsMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveWorldSettingsSnapshot({
      step: "details",
      trackCtx: inputFor(okSave, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-save", async () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    const snapshot = resolveWorldSettingsSnapshot({
      step: "saving",
      trackCtx: inputFor(save, track).trackCtx,
      save,
      track,
    });
    const actor = createActor(worldSettingsMachine, {
      input: inputFor(save, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("saving")).toBe(true);
    expect(Object.keys(actor.getSnapshot().context.changes)).toContain("details.title");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("saving")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveWorldSettingsSnapshot({
      step: "review",
      trackCtx: inputFor(okSave, track).trackCtx,
      track,
    });
    const actor = createActor(worldSettingsMachine, {
      input: inputFor(okSave, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("review")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "DISCARD" });
    expect(actor.getSnapshot().matches("details")).toBe(true);
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(WORLD_SETTINGS_EVENTS.discarded);
    expect(Object.keys(actor.getSnapshot().context.changes)).toHaveLength(0);
  });
});

describe("worldSettingsMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(worldSettingsMachine, {
      input: inputFor(okSave, () => {}),
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
    expect(ends.has("layout")).toBe(true);
    expect(ends.has("misc")).toBe(true);
    expect(ends.has("review")).toBe(true);
    expect(ends.has("saving")).toBe(true);
  });

  it("reaching review passes through every tab + REVIEW", () => {
    const paths = getShortestPaths(worldSettingsMachine, {
      input: inputFor(okSave, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const review = paths.find((p) => (p.state.value as string) === "review");
    expect(review).toBeDefined();
    const events = review!.steps.map((s) => s.event.type);
    expect(events).toContain("REVIEW");
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
    expect(events).toContain(WORLD_SETTINGS_EVENTS.changed);
    expect(events).toContain(WORLD_SETTINGS_EVENTS.reviewReached);
    expect(events).toContain(WORLD_SETTINGS_EVENTS.saving);
    expect(events).toContain(WORLD_SETTINGS_EVENTS.saved);

    expect(events.indexOf(WORLD_SETTINGS_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(WORLD_SETTINGS_EVENTS.saving),
    );
    expect(events.indexOf(WORLD_SETTINGS_EVENTS.saving)).toBeLessThan(
      events.indexOf(WORLD_SETTINGS_EVENTS.saved),
    );

    expect(actor.getSnapshot().context.result?.savedFields).toEqual(
      expect.arrayContaining(["details.title", "misc.showInPlaces"]),
    );

    const openedCall = track.mock.calls.find(
      (c) => c[0] === WORLD_SETTINGS_EVENTS.opened,
    );
    expect(openedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "ch_world_settings_wizard",
      variant: "wizard",
    });
  });

  it("discard from review drops changes and does not save", () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    const actor = createActor(worldSettingsMachine, {
      input: inputFor(save, track),
    }).start();

    actor.send({ type: "CHANGE", tab: "details", field: "description" });
    actor.send({ type: "NEXT" });
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

  it("tab-rail GO_TAB jumps directly and fires tab_viewed", () => {
    const track = vi.fn();
    const actor = createActor(worldSettingsMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "GO_TAB", tab: "misc" });
    expect(actor.getSnapshot().matches("misc")).toBe(true);
    const tabViews = track.mock.calls
      .filter((c) => c[0] === WORLD_SETTINGS_EVENTS.tabViewed)
      .map((c) => (c[1] as { tab: string }).tab);
    expect(tabViews).toContain("misc");
  });

  it("GO_TAB from review returns to the picked tab without dropping changes", () => {
    const track = vi.fn();
    const actor = createActor(worldSettingsMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "CHANGE", tab: "details", field: "title" });
    actor.send({ type: "GO_TAB", tab: "misc" });
    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("review")).toBe(true);

    actor.send({ type: "GO_TAB", tab: "layout" });
    const snap = actor.getSnapshot();
    expect(snap.matches("layout")).toBe(true);
    expect(Object.keys(snap.context.changes)).toEqual(["details.title"]);
    const discards = track.mock.calls.filter(
      (c) => c[0] === WORLD_SETTINGS_EVENTS.discarded,
    );
    expect(discards).toHaveLength(0);
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
    expect(actor.getSnapshot().context.error).toBe(
      "worlds-content-server unreachable",
    );

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("saved"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(WORLD_SETTINGS_EVENTS.saved);
  });
});

describe("simulateSave", () => {
  it("resolves the saved field list from the change set + marks itself stub:true (no network)", async () => {
    const res = await simulateSave({
      worldName: "neon-market.dcl.eth",
      changes: { "details.title": true, "misc.singlePlayer": true },
    });
    expect(res.worldName).toBe("neon-market.dcl.eth");
    expect(res.savedFields).toEqual(
      expect.arrayContaining(["details.title", "misc.singlePlayer"]),
    );
    expect(res.stub).toBe(true);
  });
});

describe("worldSettingsMachine \u{2014} stub telemetry contract", () => {
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

  it("default simulateSave emits stub:true on the saved event", async () => {
    const props = await savedEventProps(simulateSave);
    expect(props).toMatchObject({ stub: true });
  });

  it("an injected real SaveFn (stub:false) emits stub:false \u{2014} the seam is honest without machine changes", async () => {
    const realSave: SaveFn = async ({ worldName, changes }) => ({
      worldName,
      savedFields: Object.keys(changes),
      stub: false,
    });
    const props = await savedEventProps(realSave);
    expect(props).toMatchObject({ stub: false });
    expect(props.stub).not.toBe(true);
  });

  it("a SaveFn that omits stub defaults to stub:false (treated as real)", async () => {
    const impliedRealSave: SaveFn = async ({ worldName, changes }) => ({
      worldName,
      savedFields: Object.keys(changes),
    });
    const props = await savedEventProps(impliedRealSave);
    expect(props).toMatchObject({ stub: false });
  });
});
