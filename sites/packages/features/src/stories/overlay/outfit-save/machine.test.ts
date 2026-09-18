import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  outfitSaveMachine,
  OUTFIT_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveOutfitSnapshot,
  slugToState,
  stateToSlug,
  valueToSlug,
  type OutfitSaveSeed,
  type SaveFn,
  type TrackFn,
} from "./machine";

const EQUIPPED = {
  bodyShape: "urn:decentraland:off-chain:base-avatars:BaseMale",
  eyes: { color: { r: 0.2, g: 0.4, b: 0.6 } },
  hair: { color: { r: 0.3, g: 0.2, b: 0.1 } },
  skin: { color: { r: 0.8, g: 0.5, b: 0.4 } },
  wearables: [
    "urn:decentraland:off-chain:base-avatars:green_hoodie",
    "urn:decentraland:off-chain:base-avatars:brown_pants",
  ],
  forceRender: [],
};

const SEED_NO_NAME: OutfitSaveSeed = {
  equipped: EQUIPPED,
  freeSlots: 5,
  totalSlots: 10,
  namesForExtraSlots: [],
};

const SEED_WITH_NAME: OutfitSaveSeed = {
  ...SEED_NO_NAME,
  namesForExtraSlots: ["cattie"],
};

const okSave: SaveFn = async ({ slot, name }) => ({ slot, name, simulated: true });

function inputFor(seed: OutfitSaveSeed, save: SaveFn, track: TrackFn) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "bevy-overlay-outfit-save",
      variant: "wizard",
      experimentKey: "cl_outfit_save_wizard",
    },
    seed,
    save,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "browsing",
  "naming",
  "capturing",
  "saving",
  "persisting",
  "gated",
  "done",
]);

const TRAVERSAL_EVENTS = [
  { type: "OPEN_SLOT" as const, slot: 0 },
  { type: "SET_NAME" as const, name: "Beach Day" },
  { type: "NEXT" as const },
  { type: "CAPTURE" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("outfitSaveMachine \u{2014} URL ?step slug map", () => {
  it("covers every deep-linkable state (persisting presents as saving), round-trips and falls back to the first step", () => {
    const machineStates = new Set(Object.keys(outfitSaveMachine.states));
    const mappedStates = new Set(Object.keys(STATE_TO_SLUG));
    expect(machineStates).toContain("persisting");
    expect(mappedStates.has("persisting")).toBe(false);
    expect(mappedStates).toEqual(new Set([...machineStates].filter((s) => s !== "persisting")));
    expect(valueToSlug("persisting")).toBe(STATE_TO_SLUG.saving);
    expect(valueToSlug("naming")).toBe(STATE_TO_SLUG.naming);

    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
    }

    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.browsing);
    for (const missing of [null, undefined, "", "nope"]) expect(slugToState(missing)).toBe("browsing");
    expect(slugToState("name")).toBe("naming");
    expect(slugToState("capture")).toBe("capturing");
    expect(slugToState("save")).toBe("saving");
    expect(slugToState("done")).toBe("done");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("outfitSaveMachine \u{2014} deep-link hydration", () => {
  it("pins the step and seeds slot/name without telemetry or a save, and real transitions afterwards still track", async () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    expect(
      resolveOutfitSnapshot({
        step: "browsing",
        trackCtx: inputFor(SEED_NO_NAME, save, track).trackCtx,
        seed: SEED_NO_NAME,
      }),
    ).toBeUndefined();

    const capturing = createActor(outfitSaveMachine, {
      input: inputFor(SEED_NO_NAME, save, track),
      snapshot: resolveOutfitSnapshot({
        step: "capturing",
        trackCtx: inputFor(SEED_NO_NAME, save, track).trackCtx,
        seed: SEED_NO_NAME,
        save,
        track,
        slot: 1,
        name: "Beach Day",
      }),
    }).start();
    expect(capturing.getSnapshot().matches("capturing")).toBe(true);
    expect(capturing.getSnapshot().context.slot).toBe(1);
    expect(capturing.getSnapshot().context.name).toBe("Beach Day");
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();

    const naming = createActor(outfitSaveMachine, {
      input: inputFor(SEED_NO_NAME, save, track),
      snapshot: resolveOutfitSnapshot({
        step: "naming",
        trackCtx: inputFor(SEED_NO_NAME, save, track).trackCtx,
        seed: SEED_NO_NAME,
        track,
        slot: 0,
      }),
    }).start();
    expect(naming.getSnapshot().matches("naming")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    naming.send({ type: "NEXT" });
    expect(naming.getSnapshot().matches("capturing")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(OUTFIT_EVENTS.named);
  });
});

describe("outfitSaveMachine \u{2014} model-based path coverage", () => {
  it("every event-reachable path ends in an expected state and capturing is reached through OPEN_SLOT and NEXT", () => {
    const paths = getShortestPaths(outfitSaveMachine, {
      input: inputFor(SEED_WITH_NAME, okSave, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("naming")).toBe(true);
    expect(ends.has("capturing")).toBe(true);

    const capturing = paths.find((p) => (p.state.value as string) === "capturing");
    expect(capturing).toBeDefined();
    const events = capturing!.steps.map((s) => s.event.type);
    expect(events).toContain("OPEN_SLOT");
    expect(events).toContain("NEXT");
  });
});

describe("outfitSaveMachine \u{2014} happy path (free slot, simulated save)", () => {
  it("open -> name -> capture -> save fires the full funnel and lands in done, with or without a name", async () => {
    const track = vi.fn();
    const actor = createActor(outfitSaveMachine, {
      input: inputFor(SEED_NO_NAME, okSave, track),
    }).start();

    actor.send({ type: "OPEN_SLOT", slot: 0 });
    expect(actor.getSnapshot().matches("naming")).toBe(true);
    actor.send({ type: "SET_NAME", name: "Beach Day" });
    actor.send({ type: "NEXT" });
    expect(actor.getSnapshot().matches("capturing")).toBe(true);
    actor.send({ type: "CAPTURE" });

    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    for (const event of [
      OUTFIT_EVENTS.started,
      OUTFIT_EVENTS.named,
      OUTFIT_EVENTS.captured,
      OUTFIT_EVENTS.saved,
      OUTFIT_EVENTS.completed,
    ]) {
      expect(events, event).toContain(event);
    }
    expect(events).not.toContain(OUTFIT_EVENTS.gated);
    expect(events.indexOf(OUTFIT_EVENTS.started)).toBeLessThan(events.indexOf(OUTFIT_EVENTS.saved));

    expect(actor.getSnapshot().context.captured?.wearables).toEqual(EQUIPPED.wearables);
    const savedCall = track.mock.calls.find((c) => c[0] === OUTFIT_EVENTS.saved);
    expect(savedCall?.[1]).toMatchObject({ slot: 0, name: "Beach Day", simulated: true });
    expect(savedCall?.[2]).toMatchObject({ experimentKey: "cl_outfit_save_wizard" });

    const unnamed = vi.fn();
    const nameless = createActor(outfitSaveMachine, {
      input: inputFor(SEED_NO_NAME, okSave, unnamed),
    }).start();
    nameless.send({ type: "OPEN_SLOT", slot: 2 });
    nameless.send({ type: "NEXT" });
    nameless.send({ type: "CAPTURE" });
    await waitFor(nameless, (s) => s.matches("done"));
    expect(unnamed.mock.calls.map((c) => c[0])).toContain(OUTFIT_EVENTS.saved);
  });
});

describe("outfitSaveMachine \u{2014} NAME-gate (real)", () => {
  it("extra slot with NO name owned routes to `gated` (no save)", async () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    const actor = createActor(outfitSaveMachine, {
      input: inputFor(SEED_NO_NAME, save, track),
    }).start();

    actor.send({ type: "OPEN_SLOT", slot: 7 });
    actor.send({ type: "NEXT" });
    actor.send({ type: "CAPTURE" });
    await waitFor(actor, (s) => s.matches("gated"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(OUTFIT_EVENTS.gated);
    expect(events).not.toContain(OUTFIT_EVENTS.saved);
    expect(save).not.toHaveBeenCalled();
    expect(actor.getSnapshot().context.gateReason).toBe("no-name-unlock");
  });

  it("extra slot with a NAME owned is gated until the outfit is named, then saves", async () => {
    const track = vi.fn();
    const actor = createActor(outfitSaveMachine, {
      input: inputFor(SEED_WITH_NAME, okSave, track),
    }).start();
    actor.send({ type: "OPEN_SLOT", slot: 6 });
    actor.send({ type: "NEXT" });
    actor.send({ type: "CAPTURE" });
    await waitFor(actor, (s) => s.matches("gated"));
    expect(actor.getSnapshot().context.gateReason).toBe("needs-name");
    expect(track.mock.calls.map((c) => c[0])).not.toContain(OUTFIT_EVENTS.saved);

    const named = createActor(outfitSaveMachine, {
      input: inputFor(SEED_WITH_NAME, okSave, track),
    }).start();
    named.send({ type: "OPEN_SLOT", slot: 6 });
    named.send({ type: "SET_NAME", name: "Gala" });
    named.send({ type: "NEXT" });
    named.send({ type: "CAPTURE" });
    await waitFor(named, (s) => s.matches("done"));
    expect(track.mock.calls.map((c) => c[0])).toContain(OUTFIT_EVENTS.saved);
  });
});
