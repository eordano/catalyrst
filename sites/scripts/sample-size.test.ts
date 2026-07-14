import { describe, expect, it } from "vitest";

import { normInv, sampleSize } from "./sample-size";

describe("normInv (inverse normal CDF)", () => {
  it("returns the canonical z-scores", () => {
    expect(normInv(0.975)).toBeCloseTo(1.959964, 4);
    expect(normInv(0.8)).toBeCloseTo(0.841621, 4);
    expect(normInv(0.5)).toBeCloseTo(0, 6);
  });

  it("is antisymmetric about 0.5", () => {
    expect(normInv(0.9)).toBeCloseTo(-normInv(0.1), 5);
  });
});

describe("sampleSize (two-proportion)", () => {
  it("matches a known reference: p=0.20, mde=0.05, \u{3B1}=.05, power=.8", () => {
    const r = sampleSize({ baseline: 0.2, mde: 0.05 });
    expect(r.perVariant).toBeGreaterThan(950);
    expect(r.perVariant).toBeLessThan(1150);
    expect(r.total).toBe(r.perVariant * 2);
    expect(r.alpha).toBe(0.05);
    expect(r.power).toBe(0.8);
  });

  it("smaller MDE requires a larger sample (monotonic)", () => {
    const big = sampleSize({ baseline: 0.1, mde: 0.05 }).perVariant;
    const small = sampleSize({ baseline: 0.1, mde: 0.01 }).perVariant;
    expect(small).toBeGreaterThan(big);
  });

  it("higher power requires a larger sample", () => {
    const p80 = sampleSize({ baseline: 0.1, mde: 0.02, power: 0.8 }).perVariant;
    const p90 = sampleSize({ baseline: 0.1, mde: 0.02, power: 0.9 }).perVariant;
    expect(p90).toBeGreaterThan(p80);
  });

  it("returns whole numbers (rounded up)", () => {
    const r = sampleSize({ baseline: 0.12, mde: 0.02 });
    expect(Number.isInteger(r.perVariant)).toBe(true);
    expect(r.perVariant).toBeGreaterThan(0);
  });

  it("rejects out-of-range inputs", () => {
    expect(() => sampleSize({ baseline: 1.2, mde: 0.05 })).toThrow();
    expect(() => sampleSize({ baseline: 0.1, mde: 0 })).toThrow();
    expect(() => sampleSize({ baseline: 0.98, mde: 0.05 })).toThrow();
  });
});
