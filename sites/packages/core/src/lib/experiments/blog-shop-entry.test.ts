import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  BLOG_SHOP_ENTRY_ARMS,
  BLOG_SHOP_ENTRY_EXPERIMENT_KEY,
  BLOG_SHOP_ENTRY_STORY_DIR,
  BLOG_SHOP_ENTRY_TARGETS,
  activeBlogShopEntryExperiment,
  blogShopEntryFromFlags,
  blogShopItemPath,
} from "./blog-shop-entry";
import { parseStory } from "./context";

const STORY_DIR = path.join(
  process.cwd(),
  "packages",
  "features",
  "src",
  "stories",
  "misc",
  "blog-shop-entry",
);

describe("blog-shop-entry story (app/stories/misc/blog-shop-entry)", () => {
  it("parses, matches the arm config with base first, roundtrips flags, and weights base heaviest", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.key).toBe(BLOG_SHOP_ENTRY_EXPERIMENT_KEY);
    expect(meta.experiment.unit).toBe("session");
    expect(meta.status).toBe("draft");
    expect(meta.metric.primary).toBe("lp_blog_shop_open_rate");
    expect(meta.experiment.variants.map((v) => v.id)).toEqual([...BLOG_SHOP_ENTRY_ARMS]);
    expect(meta.experiment.variants[0].id).toBe("base");
    for (const v of meta.experiment.variants) {
      expect(blogShopEntryFromFlags(v.flags)).toBe(v.id);
    }
    const weights = Object.fromEntries(meta.experiment.variants.map((v) => [v.id, v.weight]));
    expect(meta.experiment.variants.reduce((a, v) => a + v.weight, 0)).toBe(100);
    expect(weights["base"]).toBeGreaterThan(weights["card"]);
    expect(weights["base"]).toBeGreaterThan(weights["rail"]);
    expect(blogShopEntryFromFlags({})).toBeNull();
    expect(blogShopEntryFromFlags({ shopEntry: 7 })).toBeNull();
    expect(blogShopEntryFromFlags({ shopEntry: "not-an-arm" })).toBeNull();
  });

  it("activeBlogShopEntryExperiment accepts the dir name, the grouped path and the key, and rejects unset or unknown values", () => {
    expect(activeBlogShopEntryExperiment("blog-shop-entry")).toBe(BLOG_SHOP_ENTRY_STORY_DIR);
    expect(activeBlogShopEntryExperiment("misc/blog-shop-entry")).toBe(BLOG_SHOP_ENTRY_STORY_DIR);
    expect(activeBlogShopEntryExperiment(" lp_blog_shop_entry ")).toBe(BLOG_SHOP_ENTRY_STORY_DIR);
    expect(activeBlogShopEntryExperiment(undefined)).toBeNull();
    expect(activeBlogShopEntryExperiment(null)).toBeNull();
    expect(activeBlogShopEntryExperiment("")).toBeNull();
    expect(activeBlogShopEntryExperiment("blog")).toBeNull();
    expect(activeBlogShopEntryExperiment("nonsense")).toBeNull();
  });

  it("targets point at real routes and item paths encode the id with provenance", () => {
    expect(BLOG_SHOP_ENTRY_TARGETS.shop).toBe("/shop?from=blog-shop-entry");
    expect(blogShopItemPath("abc")).toBe("/marketplace/abc?from=blog-shop-entry");
    expect(blogShopItemPath("0xdead:1")).toBe("/marketplace/0xdead%3A1?from=blog-shop-entry");
  });
});
