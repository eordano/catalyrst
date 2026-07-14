import { describe, expect, it } from "vitest";

import { manaToWei, weiToMana, weiToManaOrNull } from "./money";

describe("manaToWei \u{2014} canonical parseEther-exact semantics", () => {
  it("carries every decimal the user typed (no 6-decimal truncation)", () => {
    expect(manaToWei(1.2345678)).toBe("1234567800000000000");
    expect(manaToWei("1.2345678")).toBe("1234567800000000000");
    expect(manaToWei(1.2345678)).not.toBe("1234568000000000000");
  });

  it("rejects exponential-notation numbers under the strict pattern", () => {
    expect(manaToWei(5e-7)).toBe("0");
    expect(manaToWei("0.0000005")).toBe("500000000000");
  });

  it("negatives collapse to '0'", () => {
    expect(manaToWei(-1)).toBe("0");
    expect(manaToWei("-1")).toBe("0");
    expect(manaToWei(-0.5)).toBe("0");
  });

  it("zero stays '0'", () => {
    expect(manaToWei(0)).toBe("0");
    expect(manaToWei("0")).toBe("0");
  });

  it("garbage collapses to '0'", () => {
    expect(manaToWei("abc")).toBe("0");
    expect(manaToWei("")).toBe("0");
    expect(manaToWei(Number.NaN)).toBe("0");
    expect(manaToWei(Number.POSITIVE_INFINITY)).toBe("0");
  });
});

describe("weiToMana \u{2014} returns 0 on garbage", () => {
  it("converts wei strings exactly", () => {
    expect(weiToMana("1234567800000000000")).toBe(1.2345678);
    expect(weiToMana("1000000000000000000000")).toBe(1000);
  });

  it("passes zero and negatives through numerically", () => {
    expect(weiToMana("0")).toBe(0);
    expect(weiToMana("-1000000000000000000")).toBe(-1);
  });

  it("garbage yields 0", () => {
    expect(weiToMana("abc")).toBe(0);
    expect(weiToMana("")).toBe(0);
  });
});

describe("weiToManaOrNull \u{2014} null-safe display semantics", () => {
  it("positive amounts convert like weiToMana", () => {
    expect(weiToManaOrNull("1234567800000000000")).toBe(1.2345678);
  });

  it("null / undefined / empty are null", () => {
    expect(weiToManaOrNull(null)).toBeNull();
    expect(weiToManaOrNull(undefined)).toBeNull();
    expect(weiToManaOrNull("")).toBeNull();
  });

  it("zero, negatives, and garbage are null", () => {
    expect(weiToManaOrNull("0")).toBeNull();
    expect(weiToManaOrNull("-1000000000000000000")).toBeNull();
    expect(weiToManaOrNull("abc")).toBeNull();
  });
});
