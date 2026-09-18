import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  backpackMachine,
  BACKPACK_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveBackpackSnapshot,
  slugToState,
  stateToSlug,
  simulateSave,
  type SaveFn,
  type SaveResult,
  type TrackFn,
} from "./machine";

const RESULT: SaveResult = { entityId: "bafkrei-test" };

const okSave: SaveFn = async () => RESULT;

const TRACK_CTX = {
  sid: "sid-abc",
  story: "bevy-overlay-backpack-equip",
  variant: "wizard",
  experimentKey: "cl_backpack_equip",
};

function inputFor(save: SaveFn, track: TrackFn) {
  return {
    trackCtx: TRACK_CTX,
    save,
    track,
    ownedEmpty: true,
    baseWearables: [
      "urn:decentraland:off-chain:base-avatars:green_hoodie",
      "urn:decentraland:off-chain:base-avatars:brown_pants",
    ],
  };
}

const SELECT_EVT = {
  type: "SELECT" as const,
  urn: "urn:decentraland:mainnet:collections-v1:halloween_2019:witch_hat",
  category: "hat",
  rarity: "epic",
};
const EQUIP_EVT = {
  type: "EQUIP" as const,
  urn: SELECT_EVT.urn,
  slot: "hat",
  wearables: [
    "urn:decentraland:off-chain:base-avatars:green_hoodie",
    "urn:decentraland:off-chain:base-avatars:brown_pants",
    SELECT_EVT.urn,
  ],
};

const TRAVERSAL_EVENTS = [
  { type: "OPEN" as const },
  SELECT_EVT,
  { type: "INVENTORY_EMPTY" as const },
  EQUIP_EVT,
  { type: "PICK_COLOR" as const, kind: "skin" as const, color: "#f5d6c0" },
  { type: "REVIEW" as const },
  { type: "SAVE" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("backpackMachine \u{2014} URL ?step slug map", () => {
  it("maps every state to a unique round-tripping slug and falls back to opening", () => {
    const mapped = new Set(Object.keys(STATE_TO_SLUG));
    expect(mapped).toEqual(new Set(Object.keys(backpackMachine.states)));
    const slugs = Object.values(STATE_TO_SLUG);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const [state, slug] of Object.entries(STATE_TO_SLUG)) {
      expect(SLUG_TO_STATE[slug]).toBe(state);
      expect(stateToSlug(state)).toBe(slug);
      expect(slugToState(slug)).toBe(state);
    }
    expect(slugToState("browse")).toBe("browsing");
    expect(slugToState("equip")).toBe("equipping");
    expect(slugToState("color")).toBe("coloring");
    expect(slugToState("review")).toBe("reviewing");
    expect(slugToState("save")).toBe("saving");
    expect(slugToState("done")).toBe("done");
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.opening);
    for (const bad of [null, undefined, "", "nope"]) expect(slugToState(bad)).toBe("opening");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("backpackMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("boots opening without a snapshot, hydrates saving silently, and only real transitions track", async () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    expect(resolveBackpackSnapshot({ step: "opening", trackCtx: TRACK_CTX })).toBeUndefined();

    const saving = createActor(backpackMachine, {
      input: inputFor(save, track),
      snapshot: resolveBackpackSnapshot({
        step: "saving",
        trackCtx: TRACK_CTX,
        save,
        track,
        baseWearables: ["urn:decentraland:off-chain:base-avatars:green_hoodie"],
      }),
    }).start();
    expect(saving.getSnapshot().matches("saving")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
    expect(saving.getSnapshot().matches("saving")).toBe(true);

    const browsing = createActor(backpackMachine, {
      input: inputFor(okSave, track),
      snapshot: resolveBackpackSnapshot({ step: "browsing", trackCtx: TRACK_CTX, track }),
    }).start();
    expect(browsing.getSnapshot().matches("browsing")).toBe(true);
    expect(track).not.toHaveBeenCalled();
    browsing.send(SELECT_EVT);
    expect(browsing.getSnapshot().matches("selecting")).toBe(true);
    expect(names(track)).toContain(BACKPACK_EVENTS.selected);
  });
});

describe("backpackMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("the funnel states are event-reachable and reviewing needs OPEN, SELECT, EQUIP", () => {
    const paths = getShortestPaths(backpackMachine, {
      input: inputFor(okSave, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) ends.add(p.state.value as string);
    for (const s of ["browsing", "selecting", "equipping", "coloring", "reviewing", "saving"]) {
      expect(ends.has(s)).toBe(true);
    }
    const reviewing = paths.find((p) => (p.state.value as string) === "reviewing");
    const events = reviewing!.steps.map((s) => s.event.type);
    for (const e of ["OPEN", "SELECT", "EQUIP"]) expect(events).toContain(e);
  });
});

describe("backpackMachine \u{2014} telemetry events (happy path)", () => {
  it("open -> select -> equip -> color -> review -> save -> done fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(backpackMachine, { input: inputFor(okSave, track) }).start();

    actor.send({ type: "OPEN" });
    expect(actor.getSnapshot().matches("browsing")).toBe(true);
    actor.send(SELECT_EVT);
    actor.send(EQUIP_EVT);
    actor.send({ type: "PICK_COLOR", kind: "hair", color: "#b06a2c" });
    actor.send({ type: "REVIEW" });
    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = names(track);
    for (const e of [
      BACKPACK_EVENTS.opened,
      BACKPACK_EVENTS.browsed,
      BACKPACK_EVENTS.selected,
      BACKPACK_EVENTS.equipped,
      BACKPACK_EVENTS.colorChanged,
      BACKPACK_EVENTS.reviewReached,
      BACKPACK_EVENTS.saved,
      BACKPACK_EVENTS.done,
    ]) {
      expect(events).toContain(e);
    }
    expect(events.indexOf(BACKPACK_EVENTS.reviewReached)).toBeLessThan(
      events.indexOf(BACKPACK_EVENTS.saved),
    );
    const openedCall = track.mock.calls.find((c) => c[0] === BACKPACK_EVENTS.opened);
    expect(openedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "cl_backpack_equip",
      variant: "wizard",
    });
    expect(actor.getSnapshot().context.wearables).toContain(SELECT_EVT.urn);
    expect(actor.getSnapshot().context.colors.hair).toBe("#b06a2c");
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("an empty inventory stays on browse without saving, and equip can skip color straight to review", () => {
    const emptyTrack = vi.fn();
    const empty = createActor(backpackMachine, { input: inputFor(okSave, emptyTrack) }).start();
    empty.send({ type: "OPEN" });
    empty.send({ type: "INVENTORY_EMPTY" });
    expect(empty.getSnapshot().matches("browsing")).toBe(true);
    expect(names(emptyTrack)).toContain(BACKPACK_EVENTS.inventoryEmpty);
    expect(names(emptyTrack)).not.toContain(BACKPACK_EVENTS.saved);

    const track = vi.fn();
    const actor = createActor(backpackMachine, { input: inputFor(okSave, track) }).start();
    actor.send({ type: "OPEN" });
    actor.send(SELECT_EVT);
    actor.send(EQUIP_EVT);
    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);
    expect(names(track)).toContain(BACKPACK_EVENTS.reviewReached);
    expect(names(track)).not.toContain(BACKPACK_EVENTS.colorChanged);
  });
});

describe("backpackMachine \u{2014} save failure + retry", () => {
  it("save error -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const save: SaveFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("content server read-only");
      return okSave(args);
    };
    const actor = createActor(backpackMachine, { input: inputFor(save, track) }).start();

    actor.send({ type: "OPEN" });
    actor.send(SELECT_EVT);
    actor.send(EQUIP_EVT);
    actor.send({ type: "REVIEW" });
    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("content server read-only");
    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));
    expect(names(track)).toContain(BACKPACK_EVENTS.saved);
  });
});

describe("simulateSave", () => {
  it("resolves a deterministic stub entity id (no network)", async () => {
    const colors = { skin: "#c98c63", hair: "#5c3824", eye: "#3a6ea5" };
    const a = await simulateSave({ wearables: ["a", "b"], colors });
    const b = await simulateSave({ wearables: ["a", "b"], colors });
    const c = await simulateSave({ wearables: ["a", "c"], colors });
    expect(a.entityId).toBe(b.entityId);
    expect(a.entityId).not.toBe(c.entityId);
  });
});
