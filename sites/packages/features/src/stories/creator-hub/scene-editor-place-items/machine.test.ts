import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  editorMachine,
  EDITOR_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveEditorSnapshot,
  slugToState,
  stateToSlug,
  simulateSave,
  type SaveFn,
  type SaveResult,
  type TrackFn,
} from "./machine";

const RESULT: SaveResult = { composite: "main.composite", entities: 6 };

const okSave: SaveFn = async () => RESULT;

function inputFor(save: SaveFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "creator-hub-scene-editor-place-items",
      variant: "guided",
      experimentKey: "ch_editor_place_items",
    },
    save,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "editing",
  "browsingAssets",
  "placing",
  "transforming",
  "addingComponent",
  "modifying",
  "saving",
  "saved",
  "error",
]);

describe("editorMachine — URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(editorMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.editing);
    expect(slugToState(null)).toBe("editing");
    expect(slugToState(undefined)).toBe("editing");
    expect(slugToState("")).toBe("editing");
    expect(slugToState("nope")).toBe("editing");
    expect(slugToState("assets")).toBe("browsingAssets");
    expect(slugToState("place")).toBe("placing");
    expect(slugToState("transform")).toBe("transforming");
    expect(slugToState("component")).toBe("addingComponent");
    expect(slugToState("modify")).toBe("modifying");
    expect(slugToState("save")).toBe("saving");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("editorMachine — deep-link hydration (snapshot, no event replay)", () => {
  it("hydrating the first step does NOT fire ch_editor_opened (no double-fire)", async () => {
    const track = vi.fn();
    const snapshot = resolveEditorSnapshot({
      step: "editing",
      trackCtx: inputFor(okSave, track).trackCtx,
      track,
    });
    const actor = createActor(editorMachine, {
      input: inputFor(okSave, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("editing")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-save", async () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    const snapshot = resolveEditorSnapshot({
      step: "saving",
      trackCtx: inputFor(save, track).trackCtx,
      save,
      track,
      asset: { id: "oak-tree", name: "Oak Tree" },
    });
    const actor = createActor(editorMachine, {
      input: inputFor(save, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("saving")).toBe(true);
    expect(actor.getSnapshot().context.placed?.assetId).toBe("oak-tree");
    expect(actor.getSnapshot().context.component).toBe("MeshCollider");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("saving")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveEditorSnapshot({
      step: "transforming",
      trackCtx: inputFor(okSave, track).trackCtx,
      track,
    });
    const actor = createActor(editorMachine, {
      input: inputFor(okSave, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("transforming")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({ type: "SET_TRANSFORM", axis: "y" });
    expect(track.mock.calls.map((c) => c[0])).toContain(EDITOR_EVENTS.transformSet);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "BROWSE_ASSETS" as const },
  { type: "SEARCH" as const, query: "tree" },
  { type: "SELECT_ASSET" as const, id: "oak-tree", name: "Oak Tree" },
  { type: "PLACE_ASSET" as const, id: "oak-tree", name: "Oak Tree" },
  { type: "CONFIRM_ENTITY" as const, entity: 540 },
  { type: "SET_TRANSFORM" as const, axis: "y" },
  { type: "ADD_COMPONENT_PANEL" as const },
  { type: "SAVE_WITHOUT_COMPONENT" as const },
  { type: "PICK_COMPONENT" as const, component: "MeshCollider" },
  { type: "MODIFY_ENTITY" as const, entity: 517, name: "Admin Tools" },
  { type: "RENAME_ENTITY" as const, name: "Admin Tools (edited)" },
  { type: "ADD_COMPONENT" as const, component: "VisibilityComponent" },
  { type: "DELETE_ENTITY" as const },
  { type: "SAVE_EDIT" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("editorMachine — model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(editorMachine, {
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
    expect(ends.has("browsingAssets")).toBe(true);
    expect(ends.has("placing")).toBe(true);
    expect(ends.has("transforming")).toBe(true);
    expect(ends.has("addingComponent")).toBe(true);
    expect(ends.has("modifying")).toBe(true);
    expect(ends.has("saving")).toBe(true);
  });

  it("reaching saving passes through the place -> transform -> component steps", () => {
    const modifyEvents = new Set([
      "MODIFY_ENTITY",
      "RENAME_ENTITY",
      "ADD_COMPONENT",
      "DELETE_ENTITY",
      "SAVE_EDIT",
      "SAVE_WITHOUT_COMPONENT",
    ]);
    const placeEvents = TRAVERSAL_EVENTS.filter((e) => !modifyEvents.has(e.type));
    const paths = getShortestPaths(editorMachine, {
      input: inputFor(okSave, () => {}),
      events: placeEvents,
    });
    const saving = paths.find((p) => (p.state.value as string) === "saving");
    expect(saving).toBeDefined();
    const events = saving!.steps.map((s) => s.event.type);
    expect(events).toContain("BROWSE_ASSETS");
    expect(events).toContain("PLACE_ASSET");
    expect(events).toContain("CONFIRM_ENTITY");
    expect(events).toContain("ADD_COMPONENT_PANEL");
    expect(events).toContain("PICK_COMPONENT");
  });
});

describe("editorMachine — telemetry events (happy path)", () => {
  it("open -> browse -> search -> place -> transform -> add-component -> save fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(editorMachine, {
      input: inputFor(okSave, track),
    }).start();

    expect(actor.getSnapshot().matches("editing")).toBe(true);

    actor.send({ type: "BROWSE_ASSETS" });
    actor.send({ type: "SEARCH", query: "tree" });
    actor.send({ type: "PLACE_ASSET", id: "oak-tree", name: "Oak Tree" });
    expect(actor.getSnapshot().matches("placing")).toBe(true);

    actor.send({ type: "CONFIRM_ENTITY", entity: 540 });
    expect(actor.getSnapshot().matches("transforming")).toBe(true);
    expect(actor.getSnapshot().context.placed?.entity).toBe(540);

    actor.send({ type: "SET_TRANSFORM", axis: "x" });
    actor.send({ type: "SET_TRANSFORM", axis: "y" });
    actor.send({ type: "ADD_COMPONENT_PANEL" });
    actor.send({ type: "PICK_COMPONENT", component: "MeshCollider" });

    await waitFor(actor, (s) => s.matches("saved"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDITOR_EVENTS.opened);
    expect(events).toContain(EDITOR_EVENTS.assetsBrowsed);
    expect(events).toContain(EDITOR_EVENTS.assetSearched);
    expect(events).toContain(EDITOR_EVENTS.entityCreated);
    expect(events).toContain(EDITOR_EVENTS.transformSet);
    expect(events).toContain(EDITOR_EVENTS.componentAdded);
    expect(events).toContain(EDITOR_EVENTS.saved);

    expect(events.indexOf(EDITOR_EVENTS.opened)).toBeLessThan(
      events.indexOf(EDITOR_EVENTS.saved),
    );
    expect(events.indexOf(EDITOR_EVENTS.entityCreated)).toBeLessThan(
      events.indexOf(EDITOR_EVENTS.saved),
    );

    const createdCall = track.mock.calls.find(
      (c) => c[0] === EDITOR_EVENTS.entityCreated,
    );
    expect(createdCall?.[1]).toMatchObject({
      asset_id: "oak-tree",
      asset_name: "Oak Tree",
      entity: 540,
    });
    expect(createdCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "ch_editor_place_items",
      variant: "guided",
    });

    const savedCall = track.mock.calls.find((c) => c[0] === EDITOR_EVENTS.saved);
    expect(savedCall?.[1]).toMatchObject({ stub: false, component: "MeshCollider" });

    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("asset search fires per query and records the query string", () => {
    const track = vi.fn();
    const actor = createActor(editorMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "BROWSE_ASSETS" });
    actor.send({ type: "SEARCH", query: "lamp" });
    expect(actor.getSnapshot().matches("browsingAssets")).toBe(true);

    const searchCall = track.mock.calls.find(
      (c) => c[0] === EDITOR_EVENTS.assetSearched,
    );
    expect(searchCall?.[1]).toMatchObject({ query: "lamp" });
    expect(actor.getSnapshot().context.query).toBe("lamp");
  });

  it("multiple transform edits dedupe axes but fire telemetry each time", () => {
    const track = vi.fn();
    const actor = createActor(editorMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "BROWSE_ASSETS" });
    actor.send({ type: "PLACE_ASSET", id: "oak-tree", name: "Oak Tree" });
    actor.send({ type: "CONFIRM_ENTITY", entity: 540 });
    actor.send({ type: "SET_TRANSFORM", axis: "x" });
    actor.send({ type: "SET_TRANSFORM", axis: "x" });
    actor.send({ type: "SET_TRANSFORM", axis: "z" });

    expect(actor.getSnapshot().context.transformAxes).toEqual(["x", "z"]);
    const transformEvents = track.mock.calls.filter(
      (c) => c[0] === EDITOR_EVENTS.transformSet,
    );
    expect(transformEvents.length).toBe(3);
  });
});

describe("editorMachine — save placed item without a component (fewer clicks)", () => {
  it("SAVE_WITHOUT_COMPONENT from transforming saves straight away (no component)", async () => {
    const track = vi.fn();
    const actor = createActor(editorMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "BROWSE_ASSETS" });
    actor.send({ type: "PLACE_ASSET", id: "oak-tree", name: "Oak Tree" });
    actor.send({ type: "CONFIRM_ENTITY", entity: 540 });
    expect(actor.getSnapshot().matches("transforming")).toBe(true);

    actor.send({ type: "SAVE_WITHOUT_COMPONENT" });
    await waitFor(actor, (s) => s.matches("saved"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDITOR_EVENTS.entityCreated);
    expect(events).toContain(EDITOR_EVENTS.saved);
    expect(events).not.toContain(EDITOR_EVENTS.componentAdded);
    expect(actor.getSnapshot().context.component).toBeUndefined();

    const savedCall = track.mock.calls.find((c) => c[0] === EDITOR_EVENTS.saved);
    expect(savedCall?.[1]).toMatchObject({ mode: "place", entity: 540 });
    expect(savedCall?.[1].component).toBeUndefined();
  });
});

describe("editorMachine — modify phase (edit a real entity)", () => {
  it("select -> rename -> move -> add-component -> save fires the modify funnel", async () => {
    const track = vi.fn();
    const actor = createActor(editorMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "MODIFY_ENTITY", entity: 517, name: "Admin Tools" });
    expect(actor.getSnapshot().matches("modifying")).toBe(true);
    expect(actor.getSnapshot().context.selected).toEqual({
      entity: 517,
      name: "Admin Tools",
    });

    actor.send({ type: "RENAME_ENTITY", name: "Admin Console" });
    expect(actor.getSnapshot().context.modifiedName).toBe("Admin Console");

    actor.send({ type: "SET_TRANSFORM", axis: "x" });
    actor.send({ type: "ADD_COMPONENT", component: "VisibilityComponent" });
    expect(actor.getSnapshot().context.component).toBe("VisibilityComponent");

    actor.send({ type: "SAVE_EDIT" });
    await waitFor(actor, (s) => s.matches("saved"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDITOR_EVENTS.entityModified);
    expect(events).toContain(EDITOR_EVENTS.entityRenamed);
    expect(events).toContain(EDITOR_EVENTS.transformSet);
    expect(events).toContain(EDITOR_EVENTS.componentAdded);
    expect(events).toContain(EDITOR_EVENTS.saved);

    const renamedCall = track.mock.calls.find(
      (c) => c[0] === EDITOR_EVENTS.entityRenamed,
    );
    expect(renamedCall?.[1]).toMatchObject({ entity: 517, name: "Admin Console" });

    const savedCall = track.mock.calls.find((c) => c[0] === EDITOR_EVENTS.saved);
    expect(savedCall?.[1]).toMatchObject({
      mode: "modify",
      entity: 517,
      deleted: false,
      stub: false,
    });
  });

  it("delete routes through saving and tags the saved event as a deletion", async () => {
    const track = vi.fn();
    const actor = createActor(editorMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "MODIFY_ENTITY", entity: 516, name: "Video Screen" });
    actor.send({ type: "DELETE_ENTITY" });
    await waitFor(actor, (s) => s.matches("saved"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDITOR_EVENTS.entityDeleted);

    const savedCall = track.mock.calls.find((c) => c[0] === EDITOR_EVENTS.saved);
    expect(savedCall?.[1]).toMatchObject({ mode: "modify", entity: 516, deleted: true });
  });

  it("BACK from modifying returns to editing", () => {
    const track = vi.fn();
    const actor = createActor(editorMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "MODIFY_ENTITY", entity: 512, name: "theatre-data-source" });
    expect(actor.getSnapshot().matches("modifying")).toBe(true);
    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("editing")).toBe(true);
  });

  it("hydrating the modify step seeds selected without firing telemetry", async () => {
    const track = vi.fn();
    const snapshot = resolveEditorSnapshot({
      step: "modifying",
      trackCtx: inputFor(okSave, track).trackCtx,
      track,
      selected: { entity: 516, name: "Video Screen" },
    });
    const actor = createActor(editorMachine, {
      input: inputFor(okSave, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("modifying")).toBe(true);
    expect(actor.getSnapshot().context.selected).toEqual({
      entity: 516,
      name: "Video Screen",
    });
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
  });
});

describe("editorMachine — save failure + retry", () => {
  it("save error -> RETRY recovers to saved", async () => {
    const track = vi.fn();
    let calls = 0;
    const save: SaveFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("content server unreachable");
      return okSave(args);
    };

    const actor = createActor(editorMachine, {
      input: inputFor(save, track),
    }).start();

    actor.send({ type: "BROWSE_ASSETS" });
    actor.send({ type: "PLACE_ASSET", id: "oak-tree", name: "Oak Tree" });
    actor.send({ type: "CONFIRM_ENTITY", entity: 540 });
    actor.send({ type: "ADD_COMPONENT_PANEL" });
    actor.send({ type: "PICK_COMPONENT", component: "MeshCollider" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("content server unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("saved"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EDITOR_EVENTS.saved);
  });
});

describe("simulateSave", () => {
  it("resolves a composite descriptor counting the placed entity (no network)", async () => {
    const withPlaced = await simulateSave({
      placed: { entity: 540, assetId: "oak-tree", assetName: "Oak Tree" },
    });
    const empty = await simulateSave({});
    expect(withPlaced.composite).toBe("main.composite");
    expect(withPlaced.entities).toBe(6);
    expect(empty.entities).toBe(5);
  });
});

describe("editorMachine — minimal-clicks contract (place & save)", () => {
  const TARGET_EVENTS = 2;

  it(`saves a placed item without a component in <= ${TARGET_EVENTS} events`, () => {
    const placed = resolveEditorSnapshot({
      step: "placing",
      trackCtx: inputFor(okSave, () => {}).trackCtx,
      save: okSave,
      track: () => {},
    });
    const paths = getShortestPaths(editorMachine, {
      input: inputFor(okSave, () => {}),
      fromState: placed,
      events: TRAVERSAL_EVENTS,
    });
    const toSaving = paths.filter((p) => (p.state.value as string) === "saving");
    expect(toSaving.length).toBeGreaterThan(0);
    const minWeight = Math.min(...toSaving.map((p) => p.weight));
    expect(minWeight).toBeLessThanOrEqual(TARGET_EVENTS);

    const best = toSaving.reduce((a, b) => (b.weight < a.weight ? b : a));
    const events = best.steps.map((s) => s.event.type);
    expect(events).toContain("SAVE_WITHOUT_COMPONENT");
    expect(events).not.toContain("ADD_COMPONENT_PANEL");
    expect(events).not.toContain("PICK_COMPONENT");
  });
});
