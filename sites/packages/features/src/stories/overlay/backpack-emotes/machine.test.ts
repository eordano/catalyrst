import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  emotesMachine,
  EMOTES_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveEmotesSnapshot,
  resolveStep,
  slugToState,
  stateToSlug,
  simulateSave,
  type EmotesInput,
  type SaveFn,
  type SaveResult,
  type SlotBinding,
  type TrackFn,
} from "./machine";

const SEED: SlotBinding[] = [
  { slot: 1, urn: "urn:decentraland:off-chain:base-emotes:cry", name: "Cry" },
  { slot: 2, urn: "urn:decentraland:off-chain:base-emotes:dab", name: "Dab" },
];

const RESULT: SaveResult = { entityId: "bafkreitest", count: 2 };

const okSave: SaveFn = async () => RESULT;

function inputFor(save: SaveFn, track: TrackFn): EmotesInput {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "bevy-overlay-backpack-emotes",
      variant: "slot_first",
      experimentKey: "cl_backpack_emotes",
    },
    loadout: SEED,
    save,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "opening",
  "picking",
  "browsing",
  "assigning",
  "reviewing",
  "saving",
  "done",
  "error",
]);

describe("emotesMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(emotesMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.opening);
    expect(slugToState(null)).toBe("opening");
    expect(slugToState(undefined)).toBe("opening");
    expect(slugToState("")).toBe("opening");
    expect(slugToState("nope")).toBe("opening");
    expect(slugToState("browse")).toBe("browsing");
    expect(slugToState("assign")).toBe("assigning");
    expect(slugToState("review")).toBe("reviewing");
    expect(slugToState("save")).toBe("saving");
    expect(slugToState("done")).toBe("done");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });

  it("resolveStep maps the spec's step+slot URLs", () => {
    expect(resolveStep(null, null)).toBe("opening");
    expect(resolveStep(null, 3)).toBe("picking");
    expect(resolveStep(undefined, 0)).toBe("picking");
    expect(resolveStep("browse", 3)).toBe("browsing");
    expect(resolveStep("assign", 3)).toBe("assigning");
    expect(resolveStep("review", null)).toBe("reviewing");
    expect(resolveStep("save", null)).toBe("saving");
    expect(resolveStep("done", null)).toBe("done");
  });
});

describe("emotesMachine \u{2014} deep-link hydration (snapshot, no event replay)", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveEmotesSnapshot({
      step: "opening",
      trackCtx: inputFor(okSave, () => {}).trackCtx,
      loadout: SEED,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating a later step does NOT fire telemetry and does NOT auto-save", async () => {
    const track = vi.fn();
    const save = vi.fn(okSave);
    const snapshot = resolveEmotesSnapshot({
      step: "saving",
      trackCtx: inputFor(save, track).trackCtx,
      loadout: SEED,
      save,
      track,
    });
    const actor = createActor(emotesMachine, {
      input: inputFor(save, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("saving")).toBe(true);
    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("saving")).toBe(true);
  });

  it("hydrating assign seeds the slot + staged emote", () => {
    const track = vi.fn();
    const snapshot = resolveEmotesSnapshot({
      step: "assigning",
      trackCtx: inputFor(okSave, track).trackCtx,
      loadout: SEED,
      slot: 5,
      urn: "urn:decentraland:off-chain:base-emotes:wave",
      name: "Wave",
      track,
    });
    const actor = createActor(emotesMachine, {
      input: inputFor(okSave, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("assigning")).toBe(true);
    expect(actor.getSnapshot().context.activeSlot).toBe(5);
    expect(actor.getSnapshot().context.pendingUrn).toContain("wave");
    expect(track).not.toHaveBeenCalled();
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveEmotesSnapshot({
      step: "browsing",
      trackCtx: inputFor(okSave, track).trackCtx,
      loadout: SEED,
      slot: 4,
      track,
    });
    const actor = createActor(emotesMachine, {
      input: inputFor(okSave, track),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("browsing")).toBe(true);
    expect(track).not.toHaveBeenCalled();

    actor.send({
      type: "ASSIGN",
      urn: "urn:decentraland:off-chain:base-emotes:wave",
      name: "Wave",
    });
    expect(actor.getSnapshot().matches("assigning")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(EMOTES_EVENTS.assigned);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "OPEN" as const },
  { type: "PICK_SLOT" as const, slot: 3 },
  {
    type: "ASSIGN" as const,
    urn: "urn:decentraland:off-chain:base-emotes:wave",
    name: "Wave",
  },
  { type: "CONFIRM" as const },
  { type: "REVIEW" as const },
  { type: "SAVE" as const },
  { type: "BACK" as const },
  { type: "RETRY" as const },
];

describe("emotesMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(emotesMachine, {
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
    expect(ends.has("picking")).toBe(true);
    expect(ends.has("browsing")).toBe(true);
    expect(ends.has("assigning")).toBe(true);
    expect(ends.has("reviewing")).toBe(true);
    expect(ends.has("saving")).toBe(true);
  });

  it("reaching saving passes through OPEN, PICK_SLOT, REVIEW, SAVE", () => {
    const paths = getShortestPaths(emotesMachine, {
      input: inputFor(okSave, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const saving = paths.find((p) => (p.state.value as string) === "saving");
    expect(saving).toBeDefined();
    const events = saving!.steps.map((s) => s.event.type);
    expect(events).toContain("OPEN");
    expect(events).toContain("REVIEW");
    expect(events).toContain("SAVE");
  });
});

describe("emotesMachine \u{2014} telemetry events (happy path)", () => {
  it("open -> pick -> assign -> confirm -> review -> save fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(emotesMachine, {
      input: inputFor(okSave, track),
    }).start();

    actor.send({ type: "OPEN" });
    expect(actor.getSnapshot().matches("picking")).toBe(true);

    actor.send({ type: "PICK_SLOT", slot: 5 });
    expect(actor.getSnapshot().matches("browsing")).toBe(true);

    actor.send({
      type: "ASSIGN",
      urn: "urn:decentraland:off-chain:base-emotes:wave",
      name: "Wave",
    });
    expect(actor.getSnapshot().matches("assigning")).toBe(true);

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("picking")).toBe(true);
    const slot5 = actor.getSnapshot().context.loadout.find((b) => b.slot === 5);
    expect(slot5?.urn).toContain("wave");

    actor.send({ type: "REVIEW" });
    expect(actor.getSnapshot().matches("reviewing")).toBe(true);

    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EMOTES_EVENTS.started);
    expect(events).toContain(EMOTES_EVENTS.slotPicked);
    expect(events).toContain(EMOTES_EVENTS.browse);
    expect(events).toContain(EMOTES_EVENTS.assigned);
    expect(events).toContain(EMOTES_EVENTS.review);
    expect(events).toContain(EMOTES_EVENTS.saved);
    expect(events).toContain(EMOTES_EVENTS.done);

    expect(events.indexOf(EMOTES_EVENTS.started)).toBeLessThan(
      events.indexOf(EMOTES_EVENTS.saved),
    );

    const pickedCall = track.mock.calls.find((c) => c[0] === EMOTES_EVENTS.slotPicked);
    expect(pickedCall?.[1]).toMatchObject({ slot: 5 });
    const assignedCall = track.mock.calls.find((c) => c[0] === EMOTES_EVENTS.assigned);
    expect(assignedCall?.[1]).toMatchObject({ slot: 5 });
    expect((assignedCall?.[1] as { urn: string }).urn).toContain("wave");

    expect(pickedCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "cl_backpack_emotes",
      variant: "slot_first",
    });
  });
});

describe("emotesMachine \u{2014} save failure + retry", () => {
  it("save error -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const save: SaveFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("content server unreachable");
      return okSave(args);
    };

    const snapshot = resolveEmotesSnapshot({
      step: "reviewing",
      trackCtx: inputFor(save, track).trackCtx,
      loadout: SEED,
      save,
      track,
    });
    const actor = createActor(emotesMachine, {
      input: inputFor(save, track),
      snapshot,
    }).start();

    actor.send({ type: "SAVE" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("content server unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(EMOTES_EVENTS.saved);
  });
});

describe("simulateSave", () => {
  it("resolves a synthetic entity id with the loadout count (no network)", async () => {
    const out = await simulateSave({ loadout: SEED });
    expect(out.count).toBe(SEED.length);
    expect(out.entityId).toContain("simulated");
  });
});
