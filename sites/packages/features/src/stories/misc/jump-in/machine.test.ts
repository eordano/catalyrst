import { describe, expect, it, vi } from "vitest";
import { createActor, waitFor } from "xstate";
import { getShortestPaths } from "@xstate/graph";

import {
  jumpInMachine,
  JUMP_IN_EVENTS,
  type JumpInPlace,
  type LaunchFn,
  type LaunchTarget,
  type TrackFn,
} from "./machine";

const PLACE: JumpInPlace = {
  id: "place-1",
  title: "Genesis Plaza",
  base_position: "-3,-2",
  world: false,
  world_name: null,
};

const TARGET: LaunchTarget = {
  launchUrl: "https://catalyst.example.com/play/?position=-3%2C-2&realm=dcl-one",
  realm: "dcl-one",
};

const okLaunch: LaunchFn = async () => TARGET;
const failLaunch: LaunchFn = async () => {
  throw new Error("realm unreachable");
};

function inputFor(confirmStep: boolean, launch: LaunchFn, track: TrackFn) {
  return {
    place: PLACE,
    trackCtx: {
      sid: "sid-xyz",
      story: "jump-in",
      variant: confirmStep ? "treatment" : "control",
      experimentKey: "jump_in_confirm",
    },
    confirmStep,
    launch,
    track,
  };
}

const CONTROL_STATES = new Set(["idle", "launching", "launched", "error"]);
const TREATMENT_STATES = new Set([
  "idle",
  "confirming",
  "launching",
  "launched",
  "error",
]);

describe("jumpInMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("control (confirmStep:false): every path reaches an expected state; no confirming", () => {
    const paths = getShortestPaths(jumpInMachine, {
      input: inputFor(false, okLaunch, () => {}),
    });

    expect(paths.length).toBe(4);

    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(CONTROL_STATES.has(value)).toBe(true);
    }
    expect(ends.has("confirming")).toBe(false);
    expect(ends).toEqual(CONTROL_STATES);

    for (const p of paths) {
      const events = p.steps.map((s) => s.event.type);
      expect(events).not.toContain("CONFIRM");
    }
  });

  it("treatment (confirmStep:true): every path reaches an expected state; includes confirming", () => {
    const paths = getShortestPaths(jumpInMachine, {
      input: inputFor(true, okLaunch, () => {}),
    });

    expect(paths.length).toBe(5);

    const ends = new Set<string>();
    for (const p of paths) {
      const value = p.state.value as string;
      ends.add(value);
      expect(TREATMENT_STATES.has(value)).toBe(true);
    }
    expect(ends.has("confirming")).toBe(true);
    expect(ends).toEqual(TREATMENT_STATES);

    const launched = paths.find((p) => (p.state.value as string) === "launched");
    expect(launched).toBeDefined();
    const launchedEvents = launched!.steps.map((s) => s.event.type);
    expect(launchedEvents).toContain("START");
    expect(launchedEvents).toContain("CONFIRM");
  });

  it("enumerates 9 paths total across both variants", () => {
    const control = getShortestPaths(jumpInMachine, {
      input: inputFor(false, okLaunch, () => {}),
    });
    const treatment = getShortestPaths(jumpInMachine, {
      input: inputFor(true, okLaunch, () => {}),
    });
    expect(control.length + treatment.length).toBe(9);
  });
});

describe("jumpInMachine \u{2014} telemetry events (control)", () => {
  it("idle -> launching -> launched fires started + completed (no confirmed)", async () => {
    const track = vi.fn();
    const actor = createActor(jumpInMachine, {
      input: inputFor(false, okLaunch, track),
    }).start();

    actor.send({ type: "START" });
    await waitFor(actor, (s) => s.matches("launched"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(JUMP_IN_EVENTS.started);
    expect(events).toContain(JUMP_IN_EVENTS.completed);
    expect(events).not.toContain(JUMP_IN_EVENTS.confirmed);

    const startedCall = track.mock.calls.find((c) => c[0] === JUMP_IN_EVENTS.started);
    expect(startedCall?.[2]).toMatchObject({
      sid: "sid-xyz",
      experimentKey: "jump_in_confirm",
      variant: "control",
    });
    expect(actor.getSnapshot().context.target).toEqual(TARGET);
  });

  it("idle -> launching -> error fires started + failed, RETRY re-launches to launched", async () => {
    const track = vi.fn();
    let calls = 0;
    const launch: LaunchFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("realm unreachable");
      return okLaunch(args);
    };

    const actor = createActor(jumpInMachine, {
      input: inputFor(false, launch, track),
    }).start();

    actor.send({ type: "START" });
    await waitFor(actor, (s) => s.matches("error"));

    let events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(JUMP_IN_EVENTS.started);
    expect(events).toContain(JUMP_IN_EVENTS.failed);
    expect(actor.getSnapshot().context.error).toBe("realm unreachable");

    actor.send({ type: "RETRY" });
    await waitFor(actor, (s) => s.matches("launched"));

    events = track.mock.calls.map((c) => c[0]);
    expect(events).toContain(JUMP_IN_EVENTS.completed);
    expect(actor.getSnapshot().context.target).toEqual(TARGET);
  });
});

describe("jumpInMachine \u{2014} telemetry events (treatment)", () => {
  it("idle -> confirming -> launching -> launched fires started + confirmed + completed", async () => {
    const track = vi.fn();
    const actor = createActor(jumpInMachine, {
      input: inputFor(true, okLaunch, track),
    }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);

    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("launched"));

    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual([
      JUMP_IN_EVENTS.started,
      JUMP_IN_EVENTS.confirmed,
      JUMP_IN_EVENTS.completed,
    ]);
  });

  it("CANCEL from confirming returns to idle and does not launch", async () => {
    const track = vi.fn();
    const launch = vi.fn(okLaunch);
    const actor = createActor(jumpInMachine, {
      input: inputFor(true, launch, track),
    }).start();

    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);

    actor.send({ type: "CANCEL" });
    expect(actor.getSnapshot().matches("idle")).toBe(true);

    expect(launch).not.toHaveBeenCalled();
    const events = track.mock.calls.map((c) => c[0]);
    expect(events).toEqual([JUMP_IN_EVENTS.started]);
  });
});
