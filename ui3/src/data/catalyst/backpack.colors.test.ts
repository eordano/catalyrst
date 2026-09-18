import { expect, test } from "vitest";
import { color3ToHex, hexToColor3 } from "./backpack";

test("black hair swatches convert from display sRGB to the renderer's linear RGB", () => {
  const black = hexToColor3("#1c1c1c");
  expect(black.r).toBeCloseTo(0.011612, 5);
  expect(black.g).toBe(black.r);
  expect(black.b).toBe(black.r);
  expect(color3ToHex(black)).toBe("#1c1c1c");
});

test.each(["#000000", "#ffffff", "#c98c63", "#5c3824", "#3a6ea5", "#01090a"])("palette %s survives save and reload", (hex) => {
  expect(color3ToHex(hexToColor3(hex))).toBe(hex);
});

test("saved linear colors are displayed in sRGB without changing their meaning", () => {
  expect(color3ToHex({ r: 0.5, g: 0.5, b: 0.5 })).toBe("#bcbcbc");
});
