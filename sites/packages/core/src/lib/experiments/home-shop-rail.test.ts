import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  HOME_SHOP_RAIL_ARMS,
  HOME_SHOP_RAIL_EXPERIMENT_KEY,
  HOME_SHOP_RAIL_STORY_DIR,
  HOME_SHOP_RAIL_TARGETS,
  activeHomeShopRailExperiment,
  armOverride,
  homeShopItemPath,
  homeShopRailFromFlags,
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
  it("story dir exists, parses, and matches the arm config", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.key).toBe(HOME_SHOP_RAIL_EXPERIMENT_KEY);
    expect(meta.experiment.unit).toBe("session");
    expect(meta.status).toBe("draft");
    expect(meta.metric.primary).toBe("lp_home_shop_open_rate");
  });

  it("variants are exactly the three arms, base first (base = control)", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.variants.map((v) => v.id)).toEqual([
      ...HOME_SHOP_RAIL_ARMS,
    ]);
    expect(meta.experiment.variants[0].id).toBe("base");
  });

  it("every variant's flags roundtrip through homeShopRailFromFlags", () => {
    const meta = parseStory(STORY_DIR);
    for (const v of meta.experiment.variants) {
      expect(homeShopRailFromFlags(v.flags)).toBe(v.id);
    }
  });

  it("draft weights: base carries the bulk, treatments get small preview slices", () => {
    const meta = parseStory(STORY_DIR);
    const weights = Object.fromEntries(
      meta.experiment.variants.map((v) => [v.id, v.weight]),
    );
    const total = meta.experiment.variants.reduce((a, v) => a + v.weight, 0);
    expect(total).toBe(100);
    expect(weights["base"]).toBeGreaterThan(weights["cta"]);
    expect(weights["base"]).toBeGreaterThan(weights["rail"]);
  });
});

describe("activeHomeShopRailExperiment (HOME_SHOP_RAIL_EXPERIMENT env)", () => {
  it("accepts the story dir name and the experiment key", () => {
    expect(activeHomeShopRailExperiment("home-shop-rail")).toBe(
      HOME_SHOP_RAIL_STORY_DIR,
    );
    expect(activeHomeShopRailExperiment("landings/home-shop-rail")).toBe(
      HOME_SHOP_RAIL_STORY_DIR,
    );
    expect(activeHomeShopRailExperiment(" lp_home_shop_rail ")).toBe(
      HOME_SHOP_RAIL_STORY_DIR,
    );
  });

  it("rejects unset/unknown values (no experiment runs)", () => {
    expect(activeHomeShopRailExperiment(undefined)).toBeNull();
    expect(activeHomeShopRailExperiment(null)).toBeNull();
    expect(activeHomeShopRailExperiment("")).toBeNull();
    expect(activeHomeShopRailExperiment("landings/home")).toBeNull();
    expect(activeHomeShopRailExperiment("nonsense")).toBeNull();
  });
});

describe("armOverride (?arm=)", () => {
  const variants = HOME_SHOP_RAIL_ARMS.map((id) => ({ id }));

  it("returns a matching arm id", () => {
    expect(armOverride(new URL("https://x/landings/home?arm=cta"), variants)).toBe("cta");
    expect(armOverride(new URL("https://x/landings/home?arm=rail"), variants)).toBe("rail");
    expect(armOverride(new URL("https://x/landings/home?arm=base"), variants)).toBe("base");
  });

  it("ignores missing/unknown arms", () => {
    expect(armOverride(new URL("https://x/landings/home"), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/landings/home?arm="), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/landings/home?arm=bogus"), variants)).toBeUndefined();
  });
});

describe("homeShopRailFromFlags", () => {
  it("rejects non-string / unknown arms", () => {
    expect(homeShopRailFromFlags({})).toBeNull();
    expect(homeShopRailFromFlags({ shopEntry: 7 })).toBeNull();
    expect(homeShopRailFromFlags({ shopEntry: "not-an-arm" })).toBeNull();
  });
});

describe("targets", () => {
  it("point at real routes and tag provenance", () => {
    expect(HOME_SHOP_RAIL_TARGETS.shop).toBe("/shop?from=home-shop-rail");
  });

  it("homeShopItemPath encodes the id and tags provenance", () => {
    expect(homeShopItemPath("abc")).toBe("/marketplace/abc?from=home-shop-rail");
    expect(homeShopItemPath("0xdead:1")).toBe(
      "/marketplace/0xdead%3A1?from=home-shop-rail",
    );
  });
});
