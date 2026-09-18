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
const TREATMENT_STATES = new Set(["idle", "confirming", "launching", "launched", "error"]);

function names(track: ReturnType<typeof vi.fn>) {
  return track.mock.calls.map((c) => c[0]);
}

describe("jumpInMachine \u{2014} model-based path coverage (@xstate/graph)", () => {
  it("control never visits confirming; treatment reaches launched only through START then CONFIRM", () => {
    const control = getShortestPaths(jumpInMachine, {
      input: inputFor(false, okLaunch, () => {}),
    });
    expect(control.length).toBe(4);
    const controlEnds = new Set<string>();
    for (const p of control) {
      const value = p.state.value as string;
      controlEnds.add(value);
      expect(CONTROL_STATES.has(value)).toBe(true);
      expect(p.steps.map((s) => s.event.type)).not.toContain("CONFIRM");
    }
    expect(controlEnds).toEqual(CONTROL_STATES);

    const treatment = getShortestPaths(jumpInMachine, {
      input: inputFor(true, okLaunch, () => {}),
    });
    expect(treatment.length).toBe(5);
    const treatmentEnds = new Set<string>();
    for (const p of treatment) {
      const value = p.state.value as string;
      treatmentEnds.add(value);
      expect(TREATMENT_STATES.has(value)).toBe(true);
    }
    expect(treatmentEnds).toEqual(TREATMENT_STATES);
    const launched = treatment.find((p) => (p.state.value as string) === "launched");
    const launchedEvents = launched!.steps.map((s) => s.event.type);
    expect(launchedEvents).toContain("START");
    expect(launchedEvents).toContain("CONFIRM");
  });
});

describe("jumpInMachine \u{2014} telemetry events (control)", () => {
  it("START launches straight to launched (started + completed, no confirmed); a launch error fires failed and RETRY recovers", async () => {
    const track = vi.fn();
    const actor = createActor(jumpInMachine, { input: inputFor(false, okLaunch, track) }).start();
    actor.send({ type: "START" });
    await waitFor(actor, (s) => s.matches("launched"));

    const events = names(track);
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

    const failTrack = vi.fn();
    let calls = 0;
    const launch: LaunchFn = async (args) => {
      calls += 1;
      if (calls === 1) throw new Error("realm unreachable");
      return okLaunch(args);
    };
    const failing = createActor(jumpInMachine, { input: inputFor(false, launch, failTrack) }).start();
    failing.send({ type: "START" });
    await waitFor(failing, (s) => s.matches("error"));
    expect(names(failTrack)).toContain(JUMP_IN_EVENTS.started);
    expect(names(failTrack)).toContain(JUMP_IN_EVENTS.failed);
    expect(failing.getSnapshot().context.error).toBe("realm unreachable");

    failing.send({ type: "RETRY" });
    await waitFor(failing, (s) => s.matches("launched"));
    expect(names(failTrack)).toContain(JUMP_IN_EVENTS.completed);
    expect(failing.getSnapshot().context.target).toEqual(TARGET);
  });
});

describe("jumpInMachine \u{2014} telemetry events (treatment)", () => {
  it("START -> CONFIRM launches with started + confirmed + completed; CANCEL returns to idle without launching", async () => {
    const track = vi.fn();
    const actor = createActor(jumpInMachine, { input: inputFor(true, okLaunch, track) }).start();
    actor.send({ type: "START" });
    expect(actor.getSnapshot().matches("confirming")).toBe(true);
    actor.send({ type: "CONFIRM" });
    await waitFor(actor, (s) => s.matches("launched"));
    expect(names(track)).toEqual([
      JUMP_IN_EVENTS.started,
      JUMP_IN_EVENTS.confirmed,
      JUMP_IN_EVENTS.completed,
    ]);

    const cancelTrack = vi.fn();
    const launch = vi.fn(okLaunch);
    const cancelled = createActor(jumpInMachine, { input: inputFor(true, launch, cancelTrack) }).start();
    cancelled.send({ type: "START" });
    expect(cancelled.getSnapshot().matches("confirming")).toBe(true);
    cancelled.send({ type: "CANCEL" });
    expect(cancelled.getSnapshot().matches("idle")).toBe(true);
    expect(launch).not.toHaveBeenCalled();
    expect(names(cancelTrack)).toEqual([JUMP_IN_EVENTS.started]);
  });
});
