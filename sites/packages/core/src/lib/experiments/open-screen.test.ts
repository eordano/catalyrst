import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  OPEN_SCREEN_ARMS,
  OPEN_SCREEN_EXPERIMENT_KEY,
  OPEN_SCREEN_STORY_DIR,
  OPEN_SCREEN_TARGETS,
  activeOpenScreenExperiment,
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
  it("parses, matches the arm config with base first, roundtrips flags, and weights base heaviest", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.key).toBe(OPEN_SCREEN_EXPERIMENT_KEY);
    expect(meta.experiment.unit).toBe("session");
    expect(meta.experiment.variants.map((v) => v.id)).toEqual([...OPEN_SCREEN_ARMS]);
    expect(meta.experiment.variants[0].id).toBe("base");
    for (const v of meta.experiment.variants) {
      expect(openScreenFromFlags(v.flags)).toBe(v.id);
    }
    const weights = Object.fromEntries(meta.experiment.variants.map((v) => [v.id, v.weight]));
    expect(weights["base"]).toBeGreaterThan(weights["genesis"]);
    expect(weights["base"]).toBeGreaterThan(weights["three-cards"]);
    expect(openScreenFromFlags({})).toBeNull();
    expect(openScreenFromFlags({ openScreen: 7 })).toBeNull();
    expect(openScreenFromFlags({ openScreen: "not-an-arm" })).toBeNull();
  });

  it("activeOpenScreenExperiment accepts the dir name, the grouped path and the key, and rejects unset or unknown values", () => {
    expect(activeOpenScreenExperiment("open-screen")).toBe(OPEN_SCREEN_STORY_DIR);
    expect(activeOpenScreenExperiment("client/open-screen")).toBe(OPEN_SCREEN_STORY_DIR);
    expect(activeOpenScreenExperiment(" client_open_screen ")).toBe(OPEN_SCREEN_STORY_DIR);
    expect(activeOpenScreenExperiment(undefined)).toBeNull();
    expect(activeOpenScreenExperiment(null)).toBeNull();
    expect(activeOpenScreenExperiment("")).toBeNull();
    expect(activeOpenScreenExperiment("explore-open")).toBeNull();
    expect(activeOpenScreenExperiment("nonsense")).toBeNull();
  });

  it("targets point at real routes and item paths encode the id with provenance", () => {
    expect(OPEN_SCREEN_TARGETS.explore).toBe("/places");
    expect(OPEN_SCREEN_TARGETS.avatar).toMatch(/^\/bevy-overlay\/backpack-equip/);
    expect(placeJumpPath("abc")).toBe("/places/abc?from=open-screen");
    expect(placeJumpPath("a b/c")).toBe("/places/a%20b%2Fc?from=open-screen");
  });
});
