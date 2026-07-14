import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import type { MapPin } from "@data/lib/catalyst/overlay/map-jump";
import {
  mapJumpMachine,
  MAP_JUMP_EVENTS,
  STATE_TO_SLUG,
  SLUG_TO_STATE,
  FIRST_STEP_SLUG,
  resolveMapJumpSnapshot,
  slugToState,
  stateToSlug,
  simulateJump,
  type JumpFn,
  type JumpResult,
  type TrackFn,
} from "./machine";

const PIN_A: MapPin = {
  id: "place-a",
  name: "Genesis Plaza",
  coords: "0,0",
  x: 0,
  y: 0,
  category: "poi",
  users: 15,
  rating: 98,
  live: true,
  featured: true,
  creator: "Decentraland",
  world: false,
  worldName: null,
  image: null,
};

const PIN_B: MapPin = {
  ...PIN_A,
  id: "place-b",
  name: "Soul Magic",
  coords: "-45,72",
  x: -45,
  y: 72,
  category: "live",
};

const RESULT: JumpResult = { jumpUrl: "https://catalyst.example.com/play/?position=0,0" };

const okJump: JumpFn = async () => RESULT;

function inputFor(jump: JumpFn, track: TrackFn, pin: MapPin | null = null) {
  return {
    trackCtx: {
      sid: "sid-abc",
      story: "bevy-overlay-map-jump",
      variant: "navmap",
      experimentKey: "cl_map_jump",
    },
    pin,
    jump,
    track,
  };
}

const EXPECTED_STATES = new Set([
  "browsing",
  "selected",
  "confirming",
  "jumping",
  "done",
  "error",
]);

describe("mapJumpMachine \u{2014} URL ?step slug map", () => {
  it("STATE_TO_SLUG covers exactly the machine's states", () => {
    const machineStates = new Set(Object.keys(mapJumpMachine.states));
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
    expect(FIRST_STEP_SLUG).toBe(STATE_TO_SLUG.browsing);
    expect(slugToState(null)).toBe("browsing");
    expect(slugToState(undefined)).toBe("browsing");
    expect(slugToState("")).toBe("browsing");
    expect(slugToState("nope")).toBe("browsing");
    expect(slugToState("select")).toBe("selected");
    expect(slugToState("confirm")).toBe("confirming");
    expect(slugToState("jump")).toBe("jumping");
    expect(slugToState("done")).toBe("done");
    expect(stateToSlug("bogus")).toBe(FIRST_STEP_SLUG);
  });
});

describe("mapJumpMachine \u{2014} deep-link hydration", () => {
  it("first step needs no snapshot (boots from declared initial)", () => {
    const snap = resolveMapJumpSnapshot({
      step: "browsing",
      trackCtx: inputFor(okJump, () => {}).trackCtx,
    });
    expect(snap).toBeUndefined();
  });

  it("hydrating ?step=jump pins the step, fires NO machine telemetry, and does NOT auto-teleport", async () => {
    const track = vi.fn();
    const jump = vi.fn(okJump);
    const snapshot = resolveMapJumpSnapshot({
      step: "jumping",
      trackCtx: inputFor(jump, track, PIN_A).trackCtx,
      pin: PIN_A,
      jump,
      track,
    });
    const actor = createActor(mapJumpMachine, {
      input: inputFor(jump, track, PIN_A),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("jumping")).toBe(true);
    expect(actor.getSnapshot().context.pin?.id).toBe("place-a");

    await Promise.resolve();
    expect(track).not.toHaveBeenCalled();
    expect(jump).not.toHaveBeenCalled();
    expect(actor.getSnapshot().matches("jumping")).toBe(true);
  });

  it("real transitions after hydration still fire telemetry", () => {
    const track = vi.fn();
    const snapshot = resolveMapJumpSnapshot({
      step: "selected",
      trackCtx: inputFor(okJump, track, PIN_A).trackCtx,
      pin: PIN_A,
      track,
    });
    const actor = createActor(mapJumpMachine, {
      input: inputFor(okJump, track, PIN_A),
      snapshot,
    }).start();

    expect(actor.getSnapshot().matches("selected")).toBe(true);

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    expect(track.mock.calls.map((c) => c[0])).toContain(MAP_JUMP_EVENTS.confirmReached);
  });
});

const TRAVERSAL_EVENTS = [
  { type: "FILTER" as const, filter: "poi" as const },
  { type: "SELECT_PIN" as const, pin: PIN_A },
  { type: "CLEAR" as const },
  { type: "TOGGLE_HOME" as const },
  { type: "CONFIRM" as const },
  { type: "BACK" as const },
  { type: "JUMP" as const },
  { type: "RETRY" as const },
];

describe("mapJumpMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("every event-reachable path ends in an expected state", () => {
    const paths = getShortestPaths(mapJumpMachine, {
      input: inputFor(okJump, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    expect(paths.length).toBeGreaterThan(0);
    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(EXPECTED_STATES.has(value)).toBe(true);
    }
    expect(ends.has("browsing")).toBe(true);
    expect(ends.has("selected")).toBe(true);
    expect(ends.has("confirming")).toBe(true);
    expect(ends.has("jumping")).toBe(true);
  });

  it("reaching jumping passes through SELECT_PIN, CONFIRM and JUMP", () => {
    const paths = getShortestPaths(mapJumpMachine, {
      input: inputFor(okJump, () => {}),
      events: TRAVERSAL_EVENTS,
    });
    const jumping = paths.find((p) => (p.state.value as string) === "jumping");
    expect(jumping).toBeDefined();
    const events = jumping!.steps.map((s) => s.event.type);
    expect(events).toContain("SELECT_PIN");
    expect(events).toContain("CONFIRM");
    expect(events).toContain("JUMP");
  });
});

describe("mapJumpMachine \u{2014} telemetry events (happy path)", () => {
  it("filter -> select -> confirm -> jump -> done fires the full funnel", async () => {
    const track = vi.fn();
    const actor = createActor(mapJumpMachine, {
      input: inputFor(okJump, track),
    }).start();

    actor.send({ type: "FILTER", filter: "poi" });
    actor.send({ type: "SELECT_PIN", pin: PIN_A });
    expect(actor.getSnapshot().matches("selected")).toBe(true);

    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);

    actor.send({ type: "JUMP" });
    await waitFor(actor, (s) => s.matches("done"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(MAP_JUMP_EVENTS.filtered);
    expect(events).toContain(MAP_JUMP_EVENTS.pinSelected);
    expect(events).toContain(MAP_JUMP_EVENTS.confirmReached);
    expect(events).toContain(MAP_JUMP_EVENTS.jump);
    expect(events).toContain(MAP_JUMP_EVENTS.done);

    expect(events.indexOf(MAP_JUMP_EVENTS.confirmReached)).toBeLessThan(
      events.indexOf(MAP_JUMP_EVENTS.jump),
    );
    expect(events.indexOf(MAP_JUMP_EVENTS.jump)).toBeLessThanOrEqual(
      events.indexOf(MAP_JUMP_EVENTS.done),
    );

    const jumpCall = track.mock.calls.find((c) => c[0] === MAP_JUMP_EVENTS.jump);
    expect(jumpCall?.[1]).toMatchObject({
      place_id: "place-a",
      coords: "0,0",
      simulated: true,
    });
    expect(jumpCall?.[2]).toMatchObject({
      sid: "sid-abc",
      experimentKey: "cl_map_jump",
      variant: "navmap",
    });
    expect(actor.getSnapshot().context.result).toEqual(RESULT);
  });

  it("re-picking a pin in `selected` re-fires cl_map_pin_selected and updates ctx", () => {
    const track = vi.fn();
    const actor = createActor(mapJumpMachine, {
      input: inputFor(okJump, track),
    }).start();

    actor.send({ type: "SELECT_PIN", pin: PIN_A });
    actor.send({ type: "SELECT_PIN", pin: PIN_B });
    expect(actor.getSnapshot().context.pin?.id).toBe("place-b");
    const selects = track.mock.calls.filter((c) => c[0] === MAP_JUMP_EVENTS.pinSelected);
    expect(selects.length).toBe(2);
  });

  it("CONFIRM without a selected pin is a no-op (guarded)", () => {
    const track = vi.fn();
    const actor = createActor(mapJumpMachine, {
      input: inputFor(okJump, track),
    }).start();
    actor.send({ type: "SELECT_PIN", pin: PIN_A });
    actor.send({ type: "CLEAR" });
    expect(actor.getSnapshot().matches("browsing")).toBe(true);
    actor.send({ type: "CONFIRM" });
    expect(actor.getSnapshot().matches("confirming")).toBe(false);
    expect(track.mock.calls.map((c) => c[0])).not.toContain(MAP_JUMP_EVENTS.confirmReached);
  });

  it("set-as-home toggle is carried into confirm_reached + jump props", () => {
    const track = vi.fn();
    const actor = createActor(mapJumpMachine, {
      input: inputFor(okJump, track),
    }).start();
    actor.send({ type: "SELECT_PIN", pin: PIN_A });
    actor.send({ type: "TOGGLE_HOME" });
    actor.send({ type: "CONFIRM" });
    const confirmCall = track.mock.calls.find((c) => c[0] === MAP_JUMP_EVENTS.confirmReached);
    expect(confirmCall?.[1]).toMatchObject({ coords: "0,0", set_home: true });
  });
});

describe("mapJumpMachine \u{2014} teleport failure + retry", () => {
  it("teleport error -> RETRY recovers to done", async () => {
    const track = vi.fn();
    let calls = 0;
    const jump: JumpFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("bridge unavailable");
      return okJump(args);
    };
    const actor = createActor(mapJumpMachine, {
      input: inputFor(jump, track),
    }).start();

    actor.send({ type: "SELECT_PIN", pin: PIN_A });
    actor.send({ type: "CONFIRM" });
    actor.send({ type: "JUMP" });
    await waitFor(actor, (s) => s.matches("error"));
    expect(actor.getSnapshot().context.error).toBe("bridge unavailable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("done"));
    expect(track.mock.calls.map((c) => c[0])).toContain(MAP_JUMP_EVENTS.jump);
  });
});

describe("simulateJump", () => {
  it("resolves a jump URL keyed by pin (no network)", async () => {
    const parcel = await simulateJump({ pin: PIN_A });
    const world = await simulateJump({
      pin: { ...PIN_A, world: true, worldName: "my-world.dcl.eth" },
    });
    expect(parcel.jumpUrl).toContain("position=0,0");
    expect(world.jumpUrl).toContain("realm=my-world.dcl.eth");
  });
});
