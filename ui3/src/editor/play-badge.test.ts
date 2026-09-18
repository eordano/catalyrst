import { describe, expect, it } from "vitest";
import { playBadgeLabel } from "./play-badge";

describe("playBadgeLabel", () => {
  it("tells the user an empty scene has nothing to run, and keeps the short label once something is placed", () => {
    expect(playBadgeLabel(false, true)).toContain("empty scene");
    expect(playBadgeLabel(false, true)).toContain("Insert");
    expect(playBadgeLabel(true, true)).toMatch(/^\u{275A}\u{275A} Paused an empty scene/u);
    expect(playBadgeLabel(false, false)).toMatch(/Running/);
    expect(playBadgeLabel(false, false)).not.toContain("empty scene");
    expect(playBadgeLabel(true, false)).toMatch(/Paused/);
    expect(playBadgeLabel(true, false)).not.toContain("empty scene");
  });
});
