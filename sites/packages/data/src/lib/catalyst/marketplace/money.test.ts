import { describe, expect, it } from "vitest";

import { manaToWei, weiToMana, weiToManaOrNull } from "./money";

describe("manaToWei \u{2014} canonical parseEther-exact semantics", () => {
  it("carries every decimal typed, rejects exponential notation, and collapses zero, negatives and garbage to '0'", () => {
    expect(manaToWei(1.2345678)).toBe("1234567800000000000");
    expect(manaToWei("1.2345678")).toBe("1234567800000000000");
    expect(manaToWei(1.2345678)).not.toBe("1234568000000000000");

    expect(manaToWei(5e-7)).toBe("0");
    expect(manaToWei("0.0000005")).toBe("500000000000");

    expect(manaToWei(-1)).toBe("0");
    expect(manaToWei("-1")).toBe("0");
    expect(manaToWei(-0.5)).toBe("0");

    expect(manaToWei(0)).toBe("0");
    expect(manaToWei("0")).toBe("0");

    expect(manaToWei("abc")).toBe("0");
    expect(manaToWei("")).toBe("0");
    expect(manaToWei(Number.NaN)).toBe("0");
    expect(manaToWei(Number.POSITIVE_INFINITY)).toBe("0");
  });
});

describe("weiToMana \u{2014} returns 0 on garbage", () => {
  it("converts wei strings exactly, passes zero and negatives through numerically, and yields 0 for garbage", () => {
    expect(weiToMana("1234567800000000000")).toBe(1.2345678);
    expect(weiToMana("1000000000000000000000")).toBe(1000);
    expect(weiToMana("0")).toBe(0);
    expect(weiToMana("-1000000000000000000")).toBe(-1);
    expect(weiToMana("abc")).toBe(0);
    expect(weiToMana("")).toBe(0);
  });
});

describe("weiToManaOrNull \u{2014} null-safe display semantics", () => {
  it("converts positive amounts and is null for null / undefined / empty / zero / negatives / garbage", () => {
    expect(weiToManaOrNull("1234567800000000000")).toBe(1.2345678);
    expect(weiToManaOrNull(null)).toBeNull();
    expect(weiToManaOrNull(undefined)).toBeNull();
    expect(weiToManaOrNull("")).toBeNull();
    expect(weiToManaOrNull("0")).toBeNull();
    expect(weiToManaOrNull("-1000000000000000000")).toBeNull();
    expect(weiToManaOrNull("abc")).toBeNull();
  });
});
