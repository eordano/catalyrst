import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  BLOG_SHOP_ENTRY_ARMS,
  BLOG_SHOP_ENTRY_EXPERIMENT_KEY,
  BLOG_SHOP_ENTRY_STORY_DIR,
  BLOG_SHOP_ENTRY_TARGETS,
  activeBlogShopEntryExperiment,
  armOverride,
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
  it("story dir exists, parses, and matches the arm config", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.key).toBe(BLOG_SHOP_ENTRY_EXPERIMENT_KEY);
    expect(meta.experiment.unit).toBe("session");
    expect(meta.status).toBe("draft");
    expect(meta.metric.primary).toBe("lp_blog_shop_open_rate");
  });

  it("variants are exactly the three arms, base first (base = control)", () => {
    const meta = parseStory(STORY_DIR);
    expect(meta.experiment.variants.map((v) => v.id)).toEqual([
      ...BLOG_SHOP_ENTRY_ARMS,
    ]);
    expect(meta.experiment.variants[0].id).toBe("base");
  });

  it("every variant's flags roundtrip through blogShopEntryFromFlags", () => {
    const meta = parseStory(STORY_DIR);
    for (const v of meta.experiment.variants) {
      expect(blogShopEntryFromFlags(v.flags)).toBe(v.id);
    }
  });

  it("draft weights: base carries the bulk, treatments get small preview slices", () => {
    const meta = parseStory(STORY_DIR);
    const weights = Object.fromEntries(
      meta.experiment.variants.map((v) => [v.id, v.weight]),
    );
    const total = meta.experiment.variants.reduce((a, v) => a + v.weight, 0);
    expect(total).toBe(100);
    expect(weights["base"]).toBeGreaterThan(weights["card"]);
    expect(weights["base"]).toBeGreaterThan(weights["rail"]);
  });
});

describe("activeBlogShopEntryExperiment (BLOG_SHOP_ENTRY_EXPERIMENT env)", () => {
  it("accepts the story dir name and the experiment key", () => {
    expect(activeBlogShopEntryExperiment("blog-shop-entry")).toBe(
      BLOG_SHOP_ENTRY_STORY_DIR,
    );
    expect(activeBlogShopEntryExperiment("misc/blog-shop-entry")).toBe(
      BLOG_SHOP_ENTRY_STORY_DIR,
    );
    expect(activeBlogShopEntryExperiment(" lp_blog_shop_entry ")).toBe(
      BLOG_SHOP_ENTRY_STORY_DIR,
    );
  });

  it("rejects unset/unknown values (no experiment runs)", () => {
    expect(activeBlogShopEntryExperiment(undefined)).toBeNull();
    expect(activeBlogShopEntryExperiment(null)).toBeNull();
    expect(activeBlogShopEntryExperiment("")).toBeNull();
    expect(activeBlogShopEntryExperiment("blog")).toBeNull();
    expect(activeBlogShopEntryExperiment("nonsense")).toBeNull();
  });
});

describe("armOverride (?arm=)", () => {
  const variants = BLOG_SHOP_ENTRY_ARMS.map((id) => ({ id }));

  it("returns a matching arm id", () => {
    expect(armOverride(new URL("https://x/blog?arm=card"), variants)).toBe("card");
    expect(armOverride(new URL("https://x/blog?arm=rail"), variants)).toBe("rail");
    expect(armOverride(new URL("https://x/blog?arm=base"), variants)).toBe("base");
  });

  it("ignores missing/unknown arms", () => {
    expect(armOverride(new URL("https://x/blog"), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/blog?arm="), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/blog?arm=bogus"), variants)).toBeUndefined();
  });
});

describe("blogShopEntryFromFlags", () => {
  it("rejects non-string / unknown arms", () => {
    expect(blogShopEntryFromFlags({})).toBeNull();
    expect(blogShopEntryFromFlags({ shopEntry: 7 })).toBeNull();
    expect(blogShopEntryFromFlags({ shopEntry: "not-an-arm" })).toBeNull();
  });
});

describe("targets", () => {
  it("point at real routes and tag provenance", () => {
    expect(BLOG_SHOP_ENTRY_TARGETS.shop).toBe("/shop?from=blog-shop-entry");
  });

  it("blogShopItemPath encodes the id and tags provenance", () => {
    expect(blogShopItemPath("abc")).toBe("/marketplace/abc?from=blog-shop-entry");
    expect(blogShopItemPath("0xdead:1")).toBe(
      "/marketplace/0xdead%3A1?from=blog-shop-entry",
    );
  });
});
