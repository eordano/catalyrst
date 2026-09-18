import { describe, expect, it } from "vitest";
import { createEngineInput } from "./engineInput";
import type { EngineInputLogEntry } from "./engineInput";
import {
  designScale,
  movementKeys,
  resolveJoystick,
} from "./joystickGeometry";

function harness() {
  const target = document.createElement("canvas");
  document.body.appendChild(target);
  const keys: KeyboardEvent[] = [];
  const pointers: PointerEvent[] = [];
  target.addEventListener("keydown", (e) => keys.push(e));
  target.addEventListener("keyup", (e) => keys.push(e));
  for (const type of ["pointerdown", "pointerup", "pointermove"]) {
    target.addEventListener(type, (e) => pointers.push(e as PointerEvent));
  }
  const log: EngineInputLogEntry[] = [];
  const input = createEngineInput({ target, onEvent: (entry) => log.push(entry) });
  return { input, keys, pointers, log, target };
}

describe("movementKeys", () => {
  it("is empty inside the downstream dead zone, maps screen-up to forward, and sectors eight ways without pressing a near-zero axis", () => {
    expect(movementKeys({ x: 0, y: 0 }, 0.5, 0.38)).toEqual([]);
    expect(movementKeys({ x: 0.3, y: -0.3 }, 0.5, 0.38)).toEqual([]);
    expect(movementKeys({ x: 0, y: -1 }, 0.5, 0.38)).toEqual(["KeyW"]);
    expect(movementKeys({ x: 0, y: 1 }, 0.5, 0.38)).toEqual(["KeyS"]);
    expect(movementKeys({ x: -1, y: 0 }, 0.5, 0.38)).toEqual(["KeyA"]);
    expect(movementKeys({ x: 1, y: 0 }, 0.5, 0.38)).toEqual(["KeyD"]);
    expect(movementKeys({ x: 0.7, y: -0.7 }, 0.5, 0.38)).toEqual(["KeyW", "KeyD"]);
    expect(movementKeys({ x: 0.05, y: -0.9 }, 0.5, 0.38)).toEqual(["KeyW"]);
  });
});

describe("joystick geometry", () => {
  it("normalises against the 75 unit clamp radius, leaves output uncapped past it, and arms sprint only from 0.95 magnitude", () => {
    const resolved = resolveJoystick({ x: 0, y: -75 }, 75, 0.5, 0.38);
    expect(resolved.magnitude).toBeCloseTo(1);
    expect(resolved.sprintEligible).toBe(true);
    expect(resolveJoystick({ x: 150, y: 0 }, 75, 0.5, 0.38).magnitude).toBeCloseTo(2);
    expect(resolveJoystick({ x: 0, y: -70 }, 75, 0.5, 0.38).sprintEligible).toBe(false);
  });

  it("swaps the design base in portrait", () => {
    expect(designScale(1600, 720)).toBeCloseTo(1);
    expect(designScale(720, 1600)).toBeCloseTo(1);
    expect(designScale(393, 852)).toBeCloseTo(0.5325, 4);
  });
});

describe("createEngineInput", () => {
  it("presses each movement key once, releases it once, swaps keys when the direction changes, and logs a drop when the canvas is absent", () => {
    const { input, keys } = harness();
    input.setMovement(["KeyW"]);
    input.setMovement(["KeyW"]);
    input.setMovement(["KeyW"]);
    expect(keys.filter((e) => e.type === "keydown")).toHaveLength(1);
    expect(keys[0]?.code).toBe("KeyW");
    expect(keys[0]?.key).toBe("w");
    expect(keys[0]?.keyCode).toBe(87);
    input.setMovement([]);
    expect(keys.filter((e) => e.type === "keyup")).toHaveLength(1);
    input.setMovement([]);
    expect(keys.filter((e) => e.type === "keyup")).toHaveLength(1);

    input.setMovement(["KeyW", "KeyD"]);
    keys.length = 0;
    input.setMovement(["KeyS", "KeyD"]);
    expect(keys.map((e) => `${e.type}:${e.code}`)).toEqual(["keyup:KeyW", "keydown:KeyS"]);
    expect(input.heldKeys().sort()).toEqual(["KeyD", "KeyS"]);

    const log: EngineInputLogEntry[] = [];
    const orphan = createEngineInput({
      canvasId: "definitely-not-here",
      onEvent: (entry) => log.push(entry),
    });
    orphan.setMovement(["KeyW"]);
    expect(log.map((e) => e.type)).toEqual(["dropped"]);
  });

  it("releases every held key and the camera lock on releaseAll", () => {
    const { input, keys, pointers } = harness();
    input.setMovement(["KeyW"]);
    input.setModifier("ShiftLeft", true);
    input.beginLook(10, 10);
    input.look(4, 0);
    keys.length = 0;
    pointers.length = 0;
    input.releaseAll();
    expect(keys.map((e) => e.code).sort()).toEqual(["KeyW", "ShiftLeft"]);
    expect(keys.every((e) => e.type === "keyup")).toBe(true);
    expect(pointers.map((e) => e.type)).toEqual(["pointerup"]);
    expect(input.heldKeys()).toEqual([]);
  });

  it("looks as a right-button mouse drag carrying movementX/Y with a coalesced sample, honouring sensitivity and Y inversion", () => {
    const { input, pointers } = harness();
    input.beginLook(100, 100);
    input.look(12, -7);
    const move = pointers.find((e) => e.type === "pointermove");
    expect(move?.movementX).toBe(12);
    expect(move?.movementY).toBe(-7);
    expect(move?.button).toBe(-1);
    expect(move?.buttons).toBe(2);
    expect(move?.pointerType).toBe("mouse");
    expect(move?.getCoalescedEvents()).toHaveLength(1);
    input.look(5, 5);
    const downs = pointers.filter((e) => e.type === "pointerdown");
    expect(downs).toHaveLength(1);
    expect(downs[0]?.button).toBe(2);
    input.endLook();
    expect(pointers.filter((e) => e.type === "pointerup")).toHaveLength(1);

    const tuned = harness();
    tuned.input.configure({ lookSensitivity: 2, lookInvertY: true });
    tuned.input.beginLook(0, 0);
    tuned.input.look(3, 3);
    const scaled = tuned.pointers.find((e) => e.type === "pointermove");
    expect(scaled?.movementX).toBe(6);
    expect(scaled?.movementY).toBe(-6);
  });

  it("drives arrow keys under the arrow-keys look strategy and synthesises a left click for the primary tap", () => {
    const arrows = harness();
    arrows.input.configure({ lookStrategy: "arrow-keys" });
    arrows.input.beginLook(0, 0);
    arrows.input.look(5, -5);
    expect(arrows.keys.filter((e) => e.type === "keydown").map((e) => e.code).sort()).toEqual([
      "ArrowRight",
      "ArrowUp",
    ]);
    expect(arrows.pointers).toHaveLength(0);
    arrows.input.endLook();
    expect(arrows.keys.filter((e) => e.type === "keyup")).toHaveLength(2);

    const { input, pointers } = harness();
    input.primaryTap(42, 84);
    expect(pointers.map((e) => e.type)).toEqual(["pointermove", "pointerdown", "pointerup"]);
    const down = pointers[1];
    expect(down?.button).toBe(0);
    expect(down?.buttons).toBe(1);
    expect(down?.clientX).toBe(42);
    expect(down?.clientY).toBe(84);
  });
});
