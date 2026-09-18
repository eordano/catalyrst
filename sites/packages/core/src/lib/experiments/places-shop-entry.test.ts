import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  PLACES_SHOP_ENTRY_ARMS,
  PLACES_SHOP_ENTRY_EXPERIMENT_KEY,
  PLACES_SHOP_ENTRY_STORY_DIR,
  PLACES_SHOP_ENTRY_TARGETS,
  activePlacesShopEntryExperiment,
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
  it("parses, matches the arm config with base first, roundtrips flags, and weights base heaviest", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.key).toBe(PLACES_SHOP_ENTRY_EXPERIMENT_KEY);
    expect(meta.experiment.unit).toBe("session");
    expect(meta.status).toBe("draft");
    expect(meta.metric.primary).toBe("pl_shop_open_rate");
    expect(meta.experiment.variants.map((v) => v.id)).toEqual([...PLACES_SHOP_ENTRY_ARMS]);
    expect(meta.experiment.variants[0].id).toBe("base");
    for (const v of meta.experiment.variants) {
      expect(shopEntryFromFlags(v.flags)).toBe(v.id);
    }
    const weights = Object.fromEntries(meta.experiment.variants.map((v) => [v.id, v.weight]));
    expect(meta.experiment.variants.reduce((a, v) => a + v.weight, 0)).toBe(100);
    expect(weights["base"]).toBeGreaterThan(weights["pill"]);
    expect(weights["base"]).toBeGreaterThan(weights["rail"]);
    expect(shopEntryFromFlags({})).toBeNull();
    expect(shopEntryFromFlags({ shopEntry: 7 })).toBeNull();
    expect(shopEntryFromFlags({ shopEntry: "not-an-arm" })).toBeNull();
  });

  it("activePlacesShopEntryExperiment accepts the dir name, the grouped path and the key, and rejects unset or unknown values", () => {
    expect(activePlacesShopEntryExperiment("places-shop-entry")).toBe(PLACES_SHOP_ENTRY_STORY_DIR);
    expect(activePlacesShopEntryExperiment("misc/places-shop-entry")).toBe(PLACES_SHOP_ENTRY_STORY_DIR);
    expect(activePlacesShopEntryExperiment(" places_shop_entry ")).toBe(PLACES_SHOP_ENTRY_STORY_DIR);
    expect(activePlacesShopEntryExperiment(undefined)).toBeNull();
    expect(activePlacesShopEntryExperiment(null)).toBeNull();
    expect(activePlacesShopEntryExperiment("")).toBeNull();
    expect(activePlacesShopEntryExperiment("browse-places")).toBeNull();
    expect(activePlacesShopEntryExperiment("nonsense")).toBeNull();
  });

  it("targets point at real routes and item paths encode the id with provenance", () => {
    expect(PLACES_SHOP_ENTRY_TARGETS.shop).toBe("/shop?from=places-shop-entry");
    expect(shopItemPath("abc")).toBe("/marketplace/abc?from=places-shop-entry");
    expect(shopItemPath("0xdead:1")).toBe("/marketplace/0xdead%3A1?from=places-shop-entry");
  });
});
