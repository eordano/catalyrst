import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  OPEN_SCREEN_ARMS,
  OPEN_SCREEN_EXPERIMENT_KEY,
  OPEN_SCREEN_STORY_DIR,
  OPEN_SCREEN_TARGETS,
  activeOpenScreenExperiment,
  armOverride,
  openScreenFromFlags,
  placeJumpPath,
} from "./open-screen";
import { parseStory } from "./context";

const STORY_DIR = path.join(
  process.cwd(),
  "packages",
  "features",
  "src",
  "stories",
  "client",
  "open-screen",
);

describe("open-screen story (app/stories/client/open-screen)", () => {
  it("story dir exists, parses, and matches the arm config", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.key).toBe(OPEN_SCREEN_EXPERIMENT_KEY);
    expect(meta.experiment.unit).toBe("session");
    expect(meta.status).toBe("draft");
    expect(meta.metric.primary).toBe("cl_open_jumped_in_rate");
  });

  it("variants are exactly the three arms, base first (base = control)", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.variants.map((v) => v.id)).toEqual([...OPEN_SCREEN_ARMS]);
    expect(meta.experiment.variants[0].id).toBe("base");
  });

  it("every variant's flags roundtrip through openScreenFromFlags", () => {
    const meta = parseStory(STORY_DIR);
    for (const v of meta.experiment.variants) {
      expect(openScreenFromFlags(v.flags)).toBe(v.id);
    }
  });

  it("draft weights: base carries the bulk, treatments get small preview slices", () => {
    const meta = parseStory(STORY_DIR);
    const weights = Object.fromEntries(
      meta.experiment.variants.map((v) => [v.id, v.weight]),
    );
    const total = meta.experiment.variants.reduce((a, v) => a + v.weight, 0);
    expect(total).toBe(100);
    expect(weights["base"]).toBeGreaterThan(weights["genesis"]);
    expect(weights["base"]).toBeGreaterThan(weights["three-cards"]);
  });
});

describe("activeOpenScreenExperiment (OPEN_SCREEN_EXPERIMENT env)", () => {
  it("accepts the story dir name and the experiment key", () => {
    expect(activeOpenScreenExperiment("open-screen")).toBe(OPEN_SCREEN_STORY_DIR);
    expect(activeOpenScreenExperiment("client/open-screen")).toBe(OPEN_SCREEN_STORY_DIR);
    expect(activeOpenScreenExperiment(" client_open_screen ")).toBe(OPEN_SCREEN_STORY_DIR);
  });

  it("rejects unset/unknown values (no experiment runs)", () => {
    expect(activeOpenScreenExperiment(undefined)).toBeNull();
    expect(activeOpenScreenExperiment(null)).toBeNull();
    expect(activeOpenScreenExperiment("")).toBeNull();
    expect(activeOpenScreenExperiment("explore-open")).toBeNull();
    expect(activeOpenScreenExperiment("nonsense")).toBeNull();
  });
});

describe("armOverride (?arm=)", () => {
  const variants = OPEN_SCREEN_ARMS.map((id) => ({ id }));

  it("returns a matching arm id", () => {
    expect(
      armOverride(new URL("https://x/client/open-screen?arm=three-cards"), variants),
    ).toBe("three-cards");
    expect(armOverride(new URL("https://x/client/open-screen?arm=base"), variants)).toBe(
      "base",
    );
    expect(
      armOverride(new URL("https://x/client/open-screen?arm=genesis"), variants),
    ).toBe("genesis");
  });

  it("ignores missing/unknown arms", () => {
    expect(armOverride(new URL("https://x/client/open-screen"), variants)).toBeUndefined();
    expect(
      armOverride(new URL("https://x/client/open-screen?arm="), variants),
    ).toBeUndefined();
    expect(
      armOverride(new URL("https://x/client/open-screen?arm=bogus"), variants),
    ).toBeUndefined();
  });
});

describe("openScreenFromFlags", () => {
  it("rejects non-string / unknown arms", () => {
    expect(openScreenFromFlags({})).toBeNull();
    expect(openScreenFromFlags({ openScreen: 7 })).toBeNull();
    expect(openScreenFromFlags({ openScreen: "not-an-arm" })).toBeNull();
  });
});

describe("targets", () => {
  it("point at real routes", () => {
    expect(OPEN_SCREEN_TARGETS.explore).toBe("/places");
    expect(OPEN_SCREEN_TARGETS.avatar).toMatch(/^\/bevy-overlay\/backpack-equip/);
  });

  it("placeJumpPath encodes the id and tags provenance", () => {
    expect(placeJumpPath("abc")).toBe("/places/abc?from=open-screen");
    expect(placeJumpPath("a b/c")).toBe("/places/a%20b%2Fc?from=open-screen");
  });
});
