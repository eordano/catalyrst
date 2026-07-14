import { describe, it, expect } from "vitest";
import {
  nudgeFromKey,
  quatToEulerDeg,
  eulerDegToQuat,
  tidy,
  isQuat,
  type Quat,
} from "./transform-nudge";

describe("nudgeFromKey", () => {
  it("Up/Down nudge by 1", () => {
    expect(nudgeFromKey(5, "ArrowUp", false)).toBe(6);
    expect(nudgeFromKey(5, "ArrowDown", false)).toBe(4);
  });
  it("Shift+Up/Down nudge by 0.01, no float drift", () => {
    expect(nudgeFromKey(0.1, "ArrowUp", true)).toBe(0.11);
    expect(nudgeFromKey(0.3, "ArrowDown", true)).toBe(0.29);
  });
  it("ignores non-arrow keys", () => {
    expect(nudgeFromKey(5, "a", false)).toBeNull();
    expect(nudgeFromKey(5, "Enter", true)).toBeNull();
  });
  it("works on negatives", () => {
    expect(nudgeFromKey(-1, "ArrowDown", false)).toBe(-2);
    expect(nudgeFromKey(0, "ArrowDown", true)).toBe(-0.01);
  });
});

describe("quat<->euler round-trips (DCL convention)", () => {
  const approxQuat = (a: Quat, b: Quat) => {
    const same =
      Math.abs(a.x - b.x) < 1e-6 &&
      Math.abs(a.y - b.y) < 1e-6 &&
      Math.abs(a.z - b.z) < 1e-6 &&
      Math.abs(a.w - b.w) < 1e-6;
    const neg =
      Math.abs(a.x + b.x) < 1e-6 &&
      Math.abs(a.y + b.y) < 1e-6 &&
      Math.abs(a.z + b.z) < 1e-6 &&
      Math.abs(a.w + b.w) < 1e-6;
    return same || neg;
  };

  it("identity quaternion is 0,0,0 degrees", () => {
    expect(quatToEulerDeg({ x: 0, y: 0, z: 0, w: 1 })).toEqual({ x: 0, y: 0, z: 0 });
  });

  it("euler(0,0,0) -> identity quaternion", () => {
    const q = eulerDegToQuat({ x: 0, y: 0, z: 0 });
    expect(approxQuat(q, { x: 0, y: 0, z: 0, w: 1 })).toBe(true);
  });

  it("90\u{B0} yaw (Y) round-trips through quaternion and back", () => {
    const q = eulerDegToQuat({ x: 0, y: 90, z: 0 });
    const e = quatToEulerDeg(q);
    expect(e.x).toBeCloseTo(0, 4);
    expect(e.y).toBeCloseTo(90, 4);
    expect(e.z).toBeCloseTo(0, 4);
  });

  it("a general rotation round-trips euler->quat->euler", () => {
    const start = { x: 30, y: 45, z: 15 };
    const back = quatToEulerDeg(eulerDegToQuat(start));
    expect(back.x).toBeCloseTo(start.x, 3);
    expect(back.y).toBeCloseTo(start.y, 3);
    expect(back.z).toBeCloseTo(start.z, 3);
  });

  it("nudging a rotation axis by +1\u{B0} maps to a +1\u{B0} euler change", () => {
    const start = { x: 30, y: 45, z: 15 };
    const nudged = { ...start, x: nudgeFromKey(start.x, "ArrowUp", false)! };
    const back = quatToEulerDeg(eulerDegToQuat(nudged));
    expect(back.x).toBeCloseTo(31, 3);
    expect(back.y).toBeCloseTo(45, 3);
    expect(back.z).toBeCloseTo(15, 3);
  });

  it("euler is normalized to 0..360 (no negative degrees leak to the UI)", () => {
    const q = eulerDegToQuat({ x: -10, y: 0, z: 0 });
    const e = quatToEulerDeg(q);
    expect(e.x).toBeGreaterThanOrEqual(0);
    expect(e.x).toBeLessThan(360);
    expect(e.x).toBeCloseTo(350, 3);
  });
});

describe("helpers", () => {
  it("tidy strips float drift", () => {
    expect(tidy(0.1 + 0.2)).toBe(0.3);
  });
  it("isQuat detects the w component", () => {
    expect(isQuat({ x: 0, y: 0, z: 0, w: 1 })).toBe(true);
    expect(isQuat({ x: 0, y: 0, z: 0 })).toBe(false);
    expect(isQuat(null)).toBe(false);
  });
});
