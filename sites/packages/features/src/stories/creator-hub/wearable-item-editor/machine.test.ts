import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  wearableEditorMachine,
  WEARABLE_EDITOR_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveWearableEditorSnapshot,
  slugToState,
  stateToSlug,
  simulateSave,
  type WearableDraft,
  type SaveFn,
  type SaveResult,
  type TrackFn,
} from "./machine";

const RESULT: SaveResult = { itemId: "i1", urn: "urn:test:1" };

const okSave: SaveFn = async () => RESULT;

const DRAFT: WearableDraft = {
  collectionId: "c1",
  itemId: "i1",
  name: "Holographic Jacket",
  modelFile: "male/HolographicJacket.glb",
  category: "upper_body",
  rarity: "epic",
  price: "",
  free: true,
};

function inputFor(save: SaveFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "creator-wearable-item-editor",
      variant: "wizard",
      experimentKey: "bd_wearable_item_editor",
    },
    draft: DRAFT,
    save,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "selecting",
  "model",
  "category",
  "rarity",
  "price",
  "saving",
  "saved",
  "error",
]);

const TRAVERSAL_EVENTS = [
  { type: "SELECT_ITEM" as const, collectionId: "c1", itemId: "i1", name: "Holographic Jacket" },
  { type: "SET_NAME" as const, name: "Renamed Jacket" },
  { type: "SET_MODEL" as const, modelFile: "male/x.glb" },
  { type: "SET_CATEGORY" as const, category: "upper_body" },
  { type: "SET_RARITY" as const, rarity: "epic" },
  { type: "SET_PRICE" as const, price: "100", free: false },
  { type: "BACK" as const },
  { type: "REVERT" as const },
  { type: "RETRY" as const },
  { type: "ADD_ANOTHER" as const },
];

describe("wearableEditorMachine \u{2014} URL ?step slug map", () => {
  it("covers every state, round-trips uniquely, and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(wearableEditorMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(mappedStates).toEqual(machineStates);
    expect(mappedStates).toEqual(EXPECTED_STATES);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.selecting);
    for (const bad of [null, undefined, "", "nope"]) {
      expect(slugToState(bad)).toBe("selecting");
    }
    for (const same of ["model", "category", "rarity", "price"]) {
      expect(slugToState(same)).toBe(same);
    }
    expect(slugToState("save")).toBe("saving");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("wearableEditorMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot; a saving snapshot fires no telemetry and never auto-saves; a real transition after hydration fires", async () => {
    const trackCtx = inputFor(okSave, () => {}).trackCtx;
    expect(
      resolveWearableEditorSnapshot({ step: "selecting", trackCtx, draft: DRAFT }),
    ).toBeUndefined();

    const track = vi.fn();
    const save = vi.fn(okSave);
    const saving = createActor(wearableEditorMachine, {
      input: inputFor(save, track),
      snapshot: resolveWearableEditorSnapshot({ step: "saving", trackCtx, draft: DRAFT, save, track }),
    }).start();
    expect(saving.getSnapshot().matches("saving")).toBe(true);
    expect(saving.getSnapshot().context.draft.itemId).toBe("i1");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
    expect(saving.getSnapshot().matches("saving")).toBe(true);

    const rarity = createActor(wearableEditorMachine, {
      input: inputFor(okSave, track),
      snapshot: resolveWearableEditorSnapshot({ step: "rarity", trackCtx, draft: DRAFT, track }),
    }).start();
    expect(rarity.getSnapshot().matches("rarity")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    rarity.send({ type: "SET_RARITY", rarity: "legendary" });
    expect(rarity.getSnapshot().matches("price")).toBe(true);
    const raritySet = track.mock.calls.find((c) => c[0] === WEARABLE_EDITOR_EVENTS.raritySet);
    expect(raritySet).toBeDefined();
    expect(raritySet?.[1]).toMatchObject({ rarity: "legendary", max_supply: 100 });
  });
});

describe("wearableEditorMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state and saving passes through every SET_* step", () => {
    const paths = getShortestPaths(wearableEditorMachine, {
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
    for (const s of ["model", "category", "rarity", "price", "saving"]) {
      expect(ends.has(s)).toBe(true);
    }

    const saving = paths.find((p) => (p.state.value as string) === "saving");
    expect(saving).toBeDefined();
    expect(saving!.steps.map((s) => s.event.type)).toEqual(
      expect.arrayContaining(["SELECT_ITEM", "SET_MODEL", "SET_CATEGORY", "SET_RARITY", "SET_PRICE"]),
    );
  });
});

describe("wearableEditorMachine \u{2014} telemetry events (happy path)", () => {
  it("select -> model -> category -> rarity -> price -> save fires the full funnel; a free item records price 'free'", async () => {
    const track = vi.fn();
    const actor = createActor(wearableEditorMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "SELECT_ITEM", collectionId: "c1", itemId: "i1", name: "Holographic Jacket" });
    expect(actor.getSnapshot().matches("model")).toBe(true);

    actor.send({ type: "SET_MODEL", modelFile: "male/x.glb" });
    actor.send({ type: "SET_CATEGORY", category: "upper_body" });
    actor.send({ type: "SET_RARITY", rarity: "epic" });
    actor.send({ type: "SET_PRICE", price: "250", free: false });
    await waitFor(actor, (s) => s.matches("saved"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual(
      expect.arrayContaining([
        WEARABLE_EDITOR_EVENTS.opened,
        WEARABLE_EDITOR_EVENTS.modelSet,
        WEARABLE_EDITOR_EVENTS.categorySet,
        WEARABLE_EDITOR_EVENTS.raritySet,
        WEARABLE_EDITOR_EVENTS.priceSet,
        WEARABLE_EDITOR_EVENTS.saved,
        "bd_item_rarity_set",
        "bd_item_price_set",
        "bd_item_saved",
      ]),
    );
    expect(events.indexOf(WEARABLE_EDITOR_EVENTS.opened)).toBeLessThan(
      events.indexOf(WEARABLE_EDITOR_EVENTS.saved),
    );

    const raritySet = track.mock.calls.find((c) => c[0] === WEARABLE_EDITOR_EVENTS.raritySet);
    expect(raritySet?.[1]).toMatchObject({ item: "i1", rarity: "epic", max_supply: 1000 });
    const priceSet = track.mock.calls.find((c) => c[0] === WEARABLE_EDITOR_EVENTS.priceSet);
    expect(priceSet?.[1]).toMatchObject({ item: "i1", price: "250", free: false });
    const saved = track.mock.calls.find((c) => c[0] === WEARABLE_EDITOR_EVENTS.saved);
    expect(saved?.[1]).toMatchObject({ item: "i1", rarity: "epic", price: "250", stub: true });
    expect(saved?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "bd_wearable_item_editor",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);

    const freeTrack = vi.fn();
    const free = createActor(wearableEditorMachine, {
      input: inputFor(okSave, freeTrack),
    }).start();
    free.send({ type: "SELECT_ITEM", collectionId: "c1", itemId: "i1", name: "x" });
    free.send({ type: "SET_MODEL", modelFile: "m.glb" });
    free.send({ type: "SET_CATEGORY", category: "hat" });
    free.send({ type: "SET_RARITY", rarity: "rare" });
    free.send({ type: "SET_PRICE", price: "999", free: true });
    await waitFor(free, (s) => s.matches("saved"));

    const freePrice = freeTrack.mock.calls.find((c) => c[0] === WEARABLE_EDITOR_EVENTS.priceSet);
    expect(freePrice?.[1]).toMatchObject({ price: "free", free: true });
    expect(free.getSnapshot().context.draft.price).toBe("");
    expect(free.getSnapshot().context.draft.free).toBe(true);

    const out = await simulateSave({ draft: DRAFT });
    expect(out.itemId).toBe("i1");
    expect(out.urn).toContain("collections-v2");
  });
});

describe("wearableEditorMachine \u{2014} Revert (discard unsaved edits)", () => {
  it("BACK steps backwards keeping edits; REVERT from an edited step discards them back to the baseline in selecting", () => {
    const track = vi.fn();
    const actor = createActor(wearableEditorMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "SELECT_ITEM", collectionId: "c1", itemId: "i1", name: "Holographic Jacket" });
    actor.send({ type: "SET_MODEL", modelFile: "replaced.glb" });
    expect(actor.getSnapshot().matches("category")).toBe(true);

    actor.send({ type: "BACK" });
    expect(actor.getSnapshot().matches("model")).toBe(true);
    expect(actor.getSnapshot().context.draft.modelFile).toBe("replaced.glb");

    actor.send({ type: "SET_MODEL", modelFile: "replaced.glb" });
    actor.send({ type: "SET_CATEGORY", category: "hat" });
    actor.send({ type: "SET_RARITY", rarity: "common" });
    expect(actor.getSnapshot().matches("price")).toBe(true);
    expect(actor.getSnapshot().context.draft.rarity).toBe("common");

    actor.send({ type: "REVERT" });
    expect(actor.getSnapshot().matches("selecting")).toBe(true);
    expect(actor.getSnapshot().context.draft.rarity).toBe(DRAFT.rarity);
    expect(actor.getSnapshot().context.draft.modelFile).toBe(DRAFT.modelFile);
    expect(actor.getSnapshot().context.draft.category).toBe(DRAFT.category);
    expect(track.mock.calls.map((c) => c[0])).toContain(WEARABLE_EDITOR_EVENTS.reverted);
  });

  it("saved offers ADD_ANOTHER (REVERT is inert once the save committed the baseline); SET_NAME on the model step renames the draft", async () => {
    const actor = createActor(wearableEditorMachine, {
      input: inputFor(okSave, vi.fn()),
    }).start();

    actor.send({ type: "SELECT_ITEM", collectionId: "c1", itemId: "i1", name: "Holographic Jacket" });
    actor.send({ type: "SET_MODEL", modelFile: "male/x.glb" });
    actor.send({ type: "SET_CATEGORY", category: "upper_body" });
    actor.send({ type: "SET_RARITY", rarity: "legendary" });
    actor.send({ type: "SET_PRICE", price: "500", free: false });
    await waitFor(actor, (s) => s.matches("saved"));
    expect(actor.getSnapshot().context.baseline.rarity).toBe("legendary");

    actor.send({ type: "REVERT" });
    expect(actor.getSnapshot().matches("saved")).toBe(true);

    actor.send({ type: "ADD_ANOTHER" });
    expect(actor.getSnapshot().matches("selecting")).toBe(true);
    expect(actor.getSnapshot().context.draft.rarity).toBe("legendary");
    expect(actor.getSnapshot().context.draft.price).toBe("500");
    expect(actor.getSnapshot().context.result).toBeUndefined();

    const fresh = createActor(wearableEditorMachine, {
      input: inputFor(okSave, () => {}),
    }).start();
    fresh.send({ type: "SELECT_ITEM", collectionId: "c1", itemId: "new-1", name: "New wearable" });
    expect(fresh.getSnapshot().matches("model")).toBe(true);
    fresh.send({ type: "SET_NAME", name: "Chrome Visor" });
    expect(fresh.getSnapshot().matches("model")).toBe(true);
    expect(fresh.getSnapshot().context.draft.name).toBe("Chrome Visor");
    fresh.send({ type: "SET_MODEL", modelFile: "male/visor.glb" });
    expect(fresh.getSnapshot().context.draft.name).toBe("Chrome Visor");
  });
});

describe("wearableEditorMachine \u{2014} save failure + retry", () => {
  it("save error -> RETRY recovers to saved", async () => {
    const track = vi.fn();
    let calls = 0;
    const save: SaveFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("builder-server unreachable");
      return okSave(args);
    };

    const actor = createActor(wearableEditorMachine, {
      input: inputFor(save, track),
    }).start();

    actor.send({ type: "SELECT_ITEM", collectionId: "c1", itemId: "i1", name: "x" });
    actor.send({ type: "SET_MODEL", modelFile: "male/x.glb" });
    actor.send({ type: "SET_CATEGORY", category: "upper_body" });
    actor.send({ type: "SET_RARITY", rarity: "epic" });
    actor.send({ type: "SET_PRICE", price: "100", free: false });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("builder-server unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("saved"));
    expect(track.mock.calls.map((c) => c[0])).toContain(WEARABLE_EDITOR_EVENTS.saved);
  });
});
