import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  HOME_SHOP_RAIL_ARMS,
  HOME_SHOP_RAIL_EXPERIMENT_KEY,
  HOME_SHOP_RAIL_STORY_DIR,
  HOME_SHOP_RAIL_TARGETS,
  activeHomeShopRailExperiment,
  homeShopRailFromFlags,
  homeShopItemPath,
} from "./home-shop-rail";
import { parseStory } from "./context";

const STORY_DIR = path.join(
  process.cwd(),
  "packages",
  "features",
  "src",
  "stories",
  "landings",
  "home-shop-rail",
);

describe("home-shop-rail story (app/stories/landings/home-shop-rail)", () => {
  it("parses, matches the arm config with base first, roundtrips flags, and weights base heaviest", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.key).toBe(HOME_SHOP_RAIL_EXPERIMENT_KEY);
    expect(meta.experiment.unit).toBe("session");
    expect(meta.experiment.variants.map((v) => v.id)).toEqual([...HOME_SHOP_RAIL_ARMS]);
    expect(meta.experiment.variants[0].id).toBe("base");
    for (const v of meta.experiment.variants) {
      expect(homeShopRailFromFlags(v.flags)).toBe(v.id);
    }
    const weights = Object.fromEntries(meta.experiment.variants.map((v) => [v.id, v.weight]));
    expect(weights["base"]).toBeGreaterThan(weights["cta"]);
    expect(weights["base"]).toBeGreaterThan(weights["rail"]);
    expect(homeShopRailFromFlags({})).toBeNull();
    expect(homeShopRailFromFlags({ shopEntry: 7 })).toBeNull();
    expect(homeShopRailFromFlags({ shopEntry: "not-an-arm" })).toBeNull();
  });

  it("activeHomeShopRailExperiment accepts the dir name, the grouped path and the key, and rejects unset or unknown values", () => {
    expect(activeHomeShopRailExperiment("home-shop-rail")).toBe(HOME_SHOP_RAIL_STORY_DIR);
    expect(activeHomeShopRailExperiment("landings/home-shop-rail")).toBe(HOME_SHOP_RAIL_STORY_DIR);
    expect(activeHomeShopRailExperiment(" lp_home_shop_rail ")).toBe(HOME_SHOP_RAIL_STORY_DIR);
    expect(activeHomeShopRailExperiment(undefined)).toBeNull();
    expect(activeHomeShopRailExperiment(null)).toBeNull();
    expect(activeHomeShopRailExperiment("")).toBeNull();
    expect(activeHomeShopRailExperiment("landings/home")).toBeNull();
    expect(activeHomeShopRailExperiment("nonsense")).toBeNull();
  });

  it("targets point at real routes and item paths encode the id with provenance", () => {
    expect(HOME_SHOP_RAIL_TARGETS.shop).toBe("/shop?from=home-shop-rail");
    expect(homeShopItemPath("abc")).toBe("/marketplace/abc?from=home-shop-rail");
    expect(homeShopItemPath("0xdead:1")).toBe("/marketplace/0xdead%3A1?from=home-shop-rail");
  });
});
