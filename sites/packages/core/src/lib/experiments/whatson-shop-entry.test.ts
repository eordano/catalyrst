import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  WHATSON_SHOP_ENTRY_ARMS,
  WHATSON_SHOP_ENTRY_EXPERIMENT_KEY,
  WHATSON_SHOP_ENTRY_STORY_DIR,
  WHATSON_SHOP_ENTRY_TARGETS,
  activeWhatsOnShopEntryExperiment,
  armOverride,
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
  it("story dir exists, parses, and matches the arm config", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.key).toBe(WHATSON_SHOP_ENTRY_EXPERIMENT_KEY);
    expect(meta.experiment.unit).toBe("session");
    expect(meta.status).toBe("draft");
    expect(meta.metric.primary).toBe("lp_whatson_shop_open_rate");
  });

  it("variants are exactly the three arms, base first (base = control)", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.variants.map((v) => v.id)).toEqual([
      ...WHATSON_SHOP_ENTRY_ARMS,
    ]);
    expect(meta.experiment.variants[0].id).toBe("base");
  });

  it("every variant's flags roundtrip through whatsOnShopEntryFromFlags", () => {
    const meta = parseStory(STORY_DIR);
    for (const v of meta.experiment.variants) {
      expect(whatsOnShopEntryFromFlags(v.flags)).toBe(v.id);
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

describe("activeWhatsOnShopEntryExperiment (WHATSON_SHOP_ENTRY_EXPERIMENT env)", () => {
  it("accepts the story dir name and the experiment key", () => {
    expect(activeWhatsOnShopEntryExperiment("whatson-shop-entry")).toBe(
      WHATSON_SHOP_ENTRY_STORY_DIR,
    );
    expect(activeWhatsOnShopEntryExperiment("landings/whatson-shop-entry")).toBe(
      WHATSON_SHOP_ENTRY_STORY_DIR,
    );
    expect(activeWhatsOnShopEntryExperiment(" lp_whatson_shop_entry ")).toBe(
      WHATSON_SHOP_ENTRY_STORY_DIR,
    );
  });

  it("rejects unset/unknown values (no experiment runs)", () => {
    expect(activeWhatsOnShopEntryExperiment(undefined)).toBeNull();
    expect(activeWhatsOnShopEntryExperiment(null)).toBeNull();
    expect(activeWhatsOnShopEntryExperiment("")).toBeNull();
    expect(activeWhatsOnShopEntryExperiment("lp_whatson_feed")).toBeNull();
    expect(activeWhatsOnShopEntryExperiment("nonsense")).toBeNull();
  });
});

describe("armOverride (?arm=)", () => {
  const variants = WHATSON_SHOP_ENTRY_ARMS.map((id) => ({ id }));

  it("returns a matching arm id", () => {
    expect(armOverride(new URL("https://x/whats-on?arm=pill"), variants)).toBe("pill");
    expect(armOverride(new URL("https://x/whats-on?arm=rail"), variants)).toBe("rail");
    expect(armOverride(new URL("https://x/whats-on?arm=base"), variants)).toBe("base");
  });

  it("ignores missing/unknown arms", () => {
    expect(armOverride(new URL("https://x/whats-on"), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/whats-on?arm="), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/whats-on?arm=bogus"), variants)).toBeUndefined();
  });
});

describe("whatsOnShopEntryFromFlags", () => {
  it("rejects non-string / unknown arms", () => {
    expect(whatsOnShopEntryFromFlags({})).toBeNull();
    expect(whatsOnShopEntryFromFlags({ shopEntry: 7 })).toBeNull();
    expect(whatsOnShopEntryFromFlags({ shopEntry: "not-an-arm" })).toBeNull();
  });
});

describe("targets", () => {
  it("point at real routes and tag provenance", () => {
    expect(WHATSON_SHOP_ENTRY_TARGETS.shop).toBe("/shop?from=whatson-shop-entry");
  });

  it("whatsOnShopItemPath encodes the id and tags provenance", () => {
    expect(whatsOnShopItemPath("abc")).toBe("/marketplace/abc?from=whatson-shop-entry");
    expect(whatsOnShopItemPath("0xdead:1")).toBe(
      "/marketplace/0xdead%3A1?from=whatson-shop-entry",
    );
  });
});
