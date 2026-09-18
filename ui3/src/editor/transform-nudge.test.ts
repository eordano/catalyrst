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
  it("nudges by 1, by 0.01 with Shift without float drift, works on negatives, and ignores non-arrow keys", () => {
    expect(nudgeFromKey(5, "ArrowUp", false)).toBe(6);
    expect(nudgeFromKey(5, "ArrowDown", false)).toBe(4);
    expect(nudgeFromKey(0.1, "ArrowUp", true)).toBe(0.11);
    expect(nudgeFromKey(0.3, "ArrowDown", true)).toBe(0.29);
    expect(nudgeFromKey(-1, "ArrowDown", false)).toBe(-2);
    expect(nudgeFromKey(0, "ArrowDown", true)).toBe(-0.01);
    expect(nudgeFromKey(5, "a", false)).toBeNull();
    expect(nudgeFromKey(5, "Enter", true)).toBeNull();
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

  it("identity maps to 0,0,0 both ways, and a 90\u{B0} yaw and a general rotation round-trip", () => {
    expect(quatToEulerDeg({ x: 0, y: 0, z: 0, w: 1 })).toEqual({ x: 0, y: 0, z: 0 });
    expect(approxQuat(eulerDegToQuat({ x: 0, y: 0, z: 0 }), { x: 0, y: 0, z: 0, w: 1 })).toBe(true);

    const yaw = quatToEulerDeg(eulerDegToQuat({ x: 0, y: 90, z: 0 }));
    expect(yaw.x).toBeCloseTo(0, 4);
    expect(yaw.y).toBeCloseTo(90, 4);
    expect(yaw.z).toBeCloseTo(0, 4);

    const start = { x: 30, y: 45, z: 15 };
    const back = quatToEulerDeg(eulerDegToQuat(start));
    expect(back.x).toBeCloseTo(start.x, 3);
    expect(back.y).toBeCloseTo(start.y, 3);
    expect(back.z).toBeCloseTo(start.z, 3);
  });

  it("a +1\u{B0} nudge maps to a +1\u{B0} euler change, and euler is normalized to 0..360 for the UI", () => {
    const start = { x: 30, y: 45, z: 15 };
    const nudged = { ...start, x: nudgeFromKey(start.x, "ArrowUp", false)! };
    const back = quatToEulerDeg(eulerDegToQuat(nudged));
    expect(back.x).toBeCloseTo(31, 3);
    expect(back.y).toBeCloseTo(45, 3);
    expect(back.z).toBeCloseTo(15, 3);

    const e = quatToEulerDeg(eulerDegToQuat({ x: -10, y: 0, z: 0 }));
    expect(e.x).toBeGreaterThanOrEqual(0);
    expect(e.x).toBeLessThan(360);
    expect(e.x).toBeCloseTo(350, 3);
  });
});

describe("helpers", () => {
  it("tidy strips float drift and isQuat detects the w component", () => {
    expect(tidy(0.1 + 0.2)).toBe(0.3);
    expect(isQuat({ x: 0, y: 0, z: 0, w: 1 })).toBe(true);
    expect(isQuat({ x: 0, y: 0, z: 0 })).toBe(false);
    expect(isQuat(null)).toBe(false);
  });
});
