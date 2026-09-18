import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  WHATSON_SHOP_ENTRY_ARMS,
  WHATSON_SHOP_ENTRY_EXPERIMENT_KEY,
  WHATSON_SHOP_ENTRY_STORY_DIR,
  WHATSON_SHOP_ENTRY_TARGETS,
  activeWhatsOnShopEntryExperiment,
  whatsOnShopEntryFromFlags,
  whatsOnShopItemPath,
} from "./whatson-shop-entry";
import { parseStory } from "./context";

const STORY_DIR = path.join(
  process.cwd(),
  "packages",
  "features",
  "src",
  "stories",
  "landings",
  "whatson-shop-entry",
);

describe("whatson-shop-entry story (app/stories/landings/whatson-shop-entry)", () => {
  it("parses, matches the arm config with base first, roundtrips flags, and weights base heaviest", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.key).toBe(WHATSON_SHOP_ENTRY_EXPERIMENT_KEY);
    expect(meta.experiment.unit).toBe("session");
    expect(meta.experiment.variants.map((v) => v.id)).toEqual([...WHATSON_SHOP_ENTRY_ARMS]);
    expect(meta.experiment.variants[0].id).toBe("base");
    for (const v of meta.experiment.variants) {
      expect(whatsOnShopEntryFromFlags(v.flags)).toBe(v.id);
    }
    const weights = Object.fromEntries(meta.experiment.variants.map((v) => [v.id, v.weight]));
    expect(weights["base"]).toBeGreaterThan(weights["pill"]);
    expect(weights["base"]).toBeGreaterThan(weights["rail"]);
    expect(whatsOnShopEntryFromFlags({})).toBeNull();
    expect(whatsOnShopEntryFromFlags({ shopEntry: 7 })).toBeNull();
    expect(whatsOnShopEntryFromFlags({ shopEntry: "not-an-arm" })).toBeNull();
  });

  it("activeWhatsOnShopEntryExperiment accepts the dir name, the grouped path and the key, and rejects unset or unknown values", () => {
    expect(activeWhatsOnShopEntryExperiment("whatson-shop-entry")).toBe(WHATSON_SHOP_ENTRY_STORY_DIR);
    expect(activeWhatsOnShopEntryExperiment("landings/whatson-shop-entry")).toBe(WHATSON_SHOP_ENTRY_STORY_DIR);
    expect(activeWhatsOnShopEntryExperiment(" lp_whatson_shop_entry ")).toBe(WHATSON_SHOP_ENTRY_STORY_DIR);
    expect(activeWhatsOnShopEntryExperiment(undefined)).toBeNull();
    expect(activeWhatsOnShopEntryExperiment(null)).toBeNull();
    expect(activeWhatsOnShopEntryExperiment("")).toBeNull();
    expect(activeWhatsOnShopEntryExperiment("lp_whatson_feed")).toBeNull();
    expect(activeWhatsOnShopEntryExperiment("nonsense")).toBeNull();
  });

  it("targets point at real routes and item paths encode the id with provenance", () => {
    expect(WHATSON_SHOP_ENTRY_TARGETS.shop).toBe("/shop?from=whatson-shop-entry");
    expect(whatsOnShopItemPath("abc")).toBe("/marketplace/abc?from=whatson-shop-entry");
    expect(whatsOnShopItemPath("0xdead:1")).toBe("/marketplace/0xdead%3A1?from=whatson-shop-entry");
  });
});
