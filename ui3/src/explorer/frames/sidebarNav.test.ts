import { describe, expect, test } from "vitest";

import { SIDEBAR_LOWER, SIDEBAR_UPPER, sidebarGuide } from "./sidebarNav";

describe("sidebar guide copy", () => {
  test("every entry that opens the backpack says so", () => {
    const backpack = SIDEBAR_UPPER.filter((i) => i.to === "Explorer/Pages/Backpack");
    expect(backpack.length).toBeGreaterThan(0);
    for (const item of backpack) expect(item.help).toMatch(/backpack|avatar/i);
  });

  test("the guide lists every sidebar item once with non-empty help", () => {
    const guide = sidebarGuide();
    const labels = guide.map((g) => g.label);
    expect(new Set(labels).size).toBe(labels.length);
    for (const item of [...SIDEBAR_UPPER, ...SIDEBAR_LOWER]) expect(labels).toContain(item.label);
    for (const g of guide) expect(g.help.trim().length).toBeGreaterThan(0);
  });
});
