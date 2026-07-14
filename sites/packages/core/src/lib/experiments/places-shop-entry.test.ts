import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  PLACES_SHOP_ENTRY_ARMS,
  PLACES_SHOP_ENTRY_EXPERIMENT_KEY,
  PLACES_SHOP_ENTRY_STORY_DIR,
  PLACES_SHOP_ENTRY_TARGETS,
  activePlacesShopEntryExperiment,
  armOverride,
  shopEntryFromFlags,
  shopItemPath,
} from "./places-shop-entry";
import { parseStory } from "./context";

const STORY_DIR = path.join(
  process.cwd(),
  "packages",
  "features",
  "src",
  "stories",
  "misc",
  "places-shop-entry",
);

describe("places-shop-entry story (app/stories/misc/places-shop-entry)", () => {
  it("story dir exists, parses, and matches the arm config", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.key).toBe(PLACES_SHOP_ENTRY_EXPERIMENT_KEY);
    expect(meta.experiment.unit).toBe("session");
    expect(meta.status).toBe("draft");
    expect(meta.metric.primary).toBe("pl_shop_open_rate");
  });

  it("variants are exactly the three arms, base first (base = control)", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.variants.map((v) => v.id)).toEqual([
      ...PLACES_SHOP_ENTRY_ARMS,
    ]);
    expect(meta.experiment.variants[0].id).toBe("base");
  });

  it("every variant's flags roundtrip through shopEntryFromFlags", () => {
    const meta = parseStory(STORY_DIR);
    for (const v of meta.experiment.variants) {
      expect(shopEntryFromFlags(v.flags)).toBe(v.id);
    }
  });

  it("draft weights: base carries the bulk, treatments get small preview slices", () => {
    const meta = parseStory(STORY_DIR);
    const weights = Object.fromEntries(
      meta.experiment.variants.map((v) => [v.id, v.weight]),
    );
    const total = meta.experiment.variants.reduce((a, v) => a + v.weight, 0);
    expect(total).toBe(100);
    expect(weights["base"]).toBeGreaterThan(weights["pill"]);
    expect(weights["base"]).toBeGreaterThan(weights["rail"]);
  });
});

describe("activePlacesShopEntryExperiment (PLACES_SHOP_ENTRY_EXPERIMENT env)", () => {
  it("accepts the story dir name and the experiment key", () => {
    expect(activePlacesShopEntryExperiment("places-shop-entry")).toBe(
      PLACES_SHOP_ENTRY_STORY_DIR,
    );
    expect(activePlacesShopEntryExperiment("misc/places-shop-entry")).toBe(
      PLACES_SHOP_ENTRY_STORY_DIR,
    );
    expect(activePlacesShopEntryExperiment(" places_shop_entry ")).toBe(
      PLACES_SHOP_ENTRY_STORY_DIR,
    );
  });

  it("rejects unset/unknown values (no experiment runs)", () => {
    expect(activePlacesShopEntryExperiment(undefined)).toBeNull();
    expect(activePlacesShopEntryExperiment(null)).toBeNull();
    expect(activePlacesShopEntryExperiment("")).toBeNull();
    expect(activePlacesShopEntryExperiment("browse-places")).toBeNull();
    expect(activePlacesShopEntryExperiment("nonsense")).toBeNull();
  });
});

describe("armOverride (?arm=)", () => {
  const variants = PLACES_SHOP_ENTRY_ARMS.map((id) => ({ id }));

  it("returns a matching arm id", () => {
    expect(armOverride(new URL("https://x/places?arm=pill"), variants)).toBe("pill");
    expect(armOverride(new URL("https://x/places?arm=rail"), variants)).toBe("rail");
    expect(armOverride(new URL("https://x/places?arm=base"), variants)).toBe("base");
  });

  it("ignores missing/unknown arms", () => {
    expect(armOverride(new URL("https://x/places"), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/places?arm="), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/places?arm=bogus"), variants)).toBeUndefined();
  });
});

describe("shopEntryFromFlags", () => {
  it("rejects non-string / unknown arms", () => {
    expect(shopEntryFromFlags({})).toBeNull();
    expect(shopEntryFromFlags({ shopEntry: 7 })).toBeNull();
    expect(shopEntryFromFlags({ shopEntry: "not-an-arm" })).toBeNull();
  });
});

describe("targets", () => {
  it("point at real routes and tag provenance", () => {
    expect(PLACES_SHOP_ENTRY_TARGETS.shop).toBe("/shop?from=places-shop-entry");
  });

  it("shopItemPath encodes the id and tags provenance", () => {
    expect(shopItemPath("abc")).toBe("/marketplace/abc?from=places-shop-entry");
    expect(shopItemPath("0xdead:1")).toBe(
      "/marketplace/0xdead%3A1?from=places-shop-entry",
    );
  });
});
