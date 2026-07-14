import { useEffect, useRef } from "react";

import StBlogHome from "@ui/web/pages/StBlogHome";
import { blogCategories } from "@ui/data/blogCategories";
import "@ui/web/pages/stbloghome.css";

import { blogPostCards, type BlogPostCard } from "@core/lib/content/blog";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import {
  fetchCatalog,
  isCatalogItemBuyable,
  toCollectibleCard,
  type CollectibleCard,
} from "@data/lib/catalyst/marketplace/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { experimentActive } from "@core/lib/experiments/flags";
import { parseVariantOverride, storyLoader } from "@core/lib/experiments/story-loader";
import {
  BLOG_SHOP_ENTRY_ARMS,
  BLOG_SHOP_ENTRY_EXPERIMENT_KEY,
  activeBlogShopEntryExperiment,
  armOverride,
  blogShopEntryFromFlags,
  type BlogShopEntryArm,
} from "@core/lib/experiments/blog-shop-entry";
import { track, trackExposure, type TrackContext } from "@core/lib/telemetry/track";
import BlogShopEntry from "@features/stories/misc/blog-shop-entry/BlogShopEntry";

import type { Route } from "./+types/blog._index";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "misc/blog";
const SHOP_ENTRY_STORY: StoryId = "misc/blog-shop-entry";

const RAIL_ITEMS = 4;

export const handle = { agentMarkdown: "blogIndex" } satisfies AgentMarkdownHandle;

const FALLBACK: Assignment = {
  variant: "index_grid",
  flags: { mainPostHero: true },
  experimentKey: "lp_blog_index",
};

const SHOP_ENTRY_FALLBACK: Assignment = {
  variant: "base",
  flags: { shopEntry: "base" },
  experimentKey: BLOG_SHOP_ENTRY_EXPERIMENT_KEY,
};

function forcedShopArm(url: URL): BlogShopEntryArm | undefined {
  const raw =
    armOverride(url, BLOG_SHOP_ENTRY_ARMS.map((id) => ({ id }))) ??
    parseVariantOverride(url, BLOG_SHOP_ENTRY_EXPERIMENT_KEY);
  if (!raw) return undefined;
  return (BLOG_SHOP_ENTRY_ARMS as readonly string[]).includes(raw)
    ? (raw as BlogShopEntryArm)
    : undefined;
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const { sid, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  // Second experiment on this surface, its own key, assigned independently.
  // Draft until activated: a non-empty flags-service override row (or the
  // BLOG_SHOP_ENTRY_EXPERIMENT env var) turns it on; otherwise every session
  // gets base. ?arm= / ?variant=lp_blog_shop_entry:<arm> still force a preview.
  const shop = await storyLoader(request, SHOP_ENTRY_STORY, SHOP_ENTRY_FALLBACK, {
    skipExposure: true,
  });
  const shopActive = await experimentActive(BLOG_SHOP_ENTRY_EXPERIMENT_KEY, {
    envActive:
      activeBlogShopEntryExperiment(
        typeof process !== "undefined"
          ? process.env?.BLOG_SHOP_ENTRY_EXPERIMENT
          : undefined,
      ) !== null,
    user: shop.userKey,
  });
  let shopAssignment = shopActive ? shop.assignment : SHOP_ENTRY_FALLBACK;
  const forcedShop = forcedShopArm(url);
  if (forcedShop) {
    shopAssignment = {
      variant: forcedShop,
      flags: { shopEntry: forcedShop },
      experimentKey: BLOG_SHOP_ENTRY_EXPERIMENT_KEY,
    };
  }
  const shopArm: BlogShopEntryArm =
    blogShopEntryFromFlags(shopAssignment.flags) ?? "base";

  const category = url.searchParams.get("category") ?? "";
  const posts = blogPostCards(category);

  const [railItems] = await Promise.all([
    shopArm === "rail"
      ? fetchCatalog({ first: 8, isOnSale: true, sortBy: "recently_listed" })
          .then((r) =>
            r.data
              .filter(isCatalogItemBuyable)
              .slice(0, RAIL_ITEMS)
              .map((it) => toCollectibleCard(it)),
          )
          .catch(() => null)
      : Promise.resolve(null),
  ]);

  // A forced arm is QA/preview driving the surface, and an inactive experiment
  // samples nobody: neither counts as an exposure.
  if (shopActive && !forcedShop) {
    trackExposure({
      sid,
      story: SHOP_ENTRY_STORY,
      variant: shopAssignment.variant,
      experimentKey: shopAssignment.experimentKey,
    });
  }

  return wrap({
    sid,
    posts,
    category,
    shopArm,
    shopVariant: shopAssignment.variant,
    shopExperimentKey: shopAssignment.experimentKey,
    railItems,
  });
}

type LoaderData = {
  sid: string;
  posts: BlogPostCard[];
  category: string;
  shopArm: BlogShopEntryArm;
  shopVariant: string;
  shopExperimentKey: string;
  railItems: CollectibleCard[] | null;
};

export default function BlogIndexRoute({ loaderData }: Route.ComponentProps) {
  const { sid, posts, category, shopArm, shopVariant, shopExperimentKey, railItems } =
    loaderData as LoaderData;

  const shopTrackCtx: TrackContext = {
    sid,
    story: SHOP_ENTRY_STORY,
    variant: shopVariant,
    experimentKey: shopExperimentKey,
  };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("lp_blog_viewed", { post_count: posts.length }, { sid, story: STORY });
  }, [sid, posts.length]);

  function onPostClick(e: React.MouseEvent<HTMLDivElement>) {
    const anchor = (e.target as HTMLElement).closest("a");
    if (!anchor) return;
    const href = anchor.getAttribute("href") ?? "";
    const m = /^\/blog\/([^/]+)$/.exec(href);
    if (!m) return;
    track("lp_blog_post_clicked", { slug: m[1] }, { sid, story: STORY });
  }

  return (
    <div className="blog-index-route" onClickCapture={onPostClick}>
      <BlogShopEntry arm={shopArm} items={railItems} trackCtx={shopTrackCtx} />
      <StBlogHome posts={posts} categories={blogCategories(category)} />
    </div>
  );
}
