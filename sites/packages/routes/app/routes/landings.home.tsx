import { useEffect, useRef } from "react";
import { useNavigate } from "react-router";

import LdHomePage, { type LdHomeRail } from "@ui/landings/pages/LdHomePage";

import {
  eventItems,
  heroEventCards,
  hotspotCards,
  ritualCards,
  ritualItems,
  type HomeContent,
} from "@data/lib/catalyst/landings/home";
import { loadHome } from "@data/lib/catalyst/landings/home.server";
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
  HOME_SHOP_RAIL_ARMS,
  HOME_SHOP_RAIL_EXPERIMENT_KEY,
  activeHomeShopRailExperiment,
  armOverride,
  homeShopRailFromFlags,
  type HomeShopRailArm,
} from "@core/lib/experiments/home-shop-rail";
import { track, trackExposure, type TrackContext } from "@core/lib/telemetry/track";
import HomeShopRail from "@features/stories/landings/home-shop-rail/HomeShopRail";

import type { Route } from "./+types/landings.home";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/home";
const SHOP_STORY: StoryId = "landings/home-shop-rail";

const RAIL_ITEMS = 6;

const FALLBACK: Assignment = {
  variant: "hero_cta",
  flags: { heroDownloadCta: true, showFeatureRails: true },
  experimentKey: "lp_home_hero_cta",
};

const HOME_SHOP_RAIL_FALLBACK: Assignment = {
  variant: "base",
  flags: { shopEntry: "base" },
  experimentKey: HOME_SHOP_RAIL_EXPERIMENT_KEY,
};

function forcedShopArm(url: URL): HomeShopRailArm | undefined {
  const raw =
    armOverride(url, HOME_SHOP_RAIL_ARMS.map((id) => ({ id }))) ??
    parseVariantOverride(url, HOME_SHOP_RAIL_EXPERIMENT_KEY);
  if (!raw) return undefined;
  return (HOME_SHOP_RAIL_ARMS as readonly string[]).includes(raw)
    ? (raw as HomeShopRailArm)
    : undefined;
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  // Second experiment on this surface, its own key, assigned independently of
  // the running lp_home_hero_cta experiment. Draft until activated: a
  // non-empty flags-service override row (or the HOME_SHOP_RAIL_EXPERIMENT env
  // var) turns it on; otherwise every session gets base. ?arm= /
  // ?variant=lp_home_shop_rail:<arm> still force a preview.
  const shop = await storyLoader(request, SHOP_STORY, HOME_SHOP_RAIL_FALLBACK, {
    skipExposure: true,
  });
  const shopActive = await experimentActive(HOME_SHOP_RAIL_EXPERIMENT_KEY, {
    envActive:
      activeHomeShopRailExperiment(
        typeof process !== "undefined"
          ? process.env?.HOME_SHOP_RAIL_EXPERIMENT
          : undefined,
      ) !== null,
    user: shop.userKey,
  });
  let shopAssignment = shopActive ? shop.assignment : HOME_SHOP_RAIL_FALLBACK;
  const forcedShop = forcedShopArm(url);
  if (forcedShop) {
    shopAssignment = {
      variant: forcedShop,
      flags: { shopEntry: forcedShop },
      experimentKey: HOME_SHOP_RAIL_EXPERIMENT_KEY,
    };
  }
  const shopArm: HomeShopRailArm =
    homeShopRailFromFlags(shopAssignment.flags) ?? "base";

  const [{ content, live }, railItems] = await Promise.all([
    loadHome(),
    shopArm === "rail"
      ? fetchCatalog({ first: 12, isOnSale: true, sortBy: "recently_listed" })
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
      story: SHOP_STORY,
      variant: shopAssignment.variant,
      experimentKey: shopAssignment.experimentKey,
    });
  }

  const payload = {
    sid,
    content,
    live,
    shopArm,
    shopVariant: shopAssignment.variant,
    shopExperimentKey: shopAssignment.experimentKey,
    railItems,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  content: HomeContent;
  live: boolean;
  shopArm: HomeShopRailArm;
  shopVariant: string;
  shopExperimentKey: string;
  railItems: CollectibleCard[] | null;
};

export default function LandingsHome({ loaderData }: Route.ComponentProps) {
  const d = loaderData as LoaderData;
  return (
    <HomeLanding
      sid={d.sid}
      content={d.content}
      live={d.live}
      shopArm={d.shopArm}
      railItems={d.railItems}
      shopTrackCtx={{
        sid: d.sid,
        story: SHOP_STORY,
        variant: d.shopVariant,
        experimentKey: d.shopExperimentKey,
      }}
    />
  );
}

function toRails(content: HomeContent): LdHomeRail[] {
  return content.rails.map((rail) => ({
    id: rail.id,
    title: rail.title,
    kind: rail.kind,
    viewAll: rail.viewAll,
    items:
      rail.kind === "events"
        ? eventItems(rail).map((ev) => ({
            id: ev.id,
            title: ev.title,
            href: `/whats-on/${encodeURIComponent(ev.id)}`,
            category: ev.category,
            live: ev.live,
            when: ev.when,
          }))
        : rail.kind === "rituals"
          ? ritualItems(rail).map((r) => ({ id: r.id, title: r.title, href: r.href }))
          : [],
  }));
}

type LandingProps = {
  sid: string;
  content: HomeContent;
  live: boolean;
  shopArm: HomeShopRailArm;
  railItems: CollectibleCard[] | null;
  shopTrackCtx: TrackContext;
};

function HomeLanding({
  sid,
  content,
  live,
  shopArm,
  railItems,
  shopTrackCtx,
}: LandingProps) {
  const navigate = useNavigate();
  const railCount = content.rails.length;

  useHomeView(sid, railCount, live);

  const railsRef = useRailExposure(sid, content);

  function onDownloadClick(store: string, placement: string) {
    track(
      "lp_home_download_clicked",
      { store, placement },
      { sid, story: STORY },
    );
  }

  function onRailClick(rail: string, target: string) {
    track("lp_home_rail_clicked", { rail, target }, { sid, story: STORY });
  }

  return (
    <LdHomePage
      hero={content.hero}
      rails={toRails(content)}
      comeHangOutTitle={content.comeHangOut.title}
      live={live}
      events={heroEventCards(content)}
      hotspots={hotspotCards(content)}
      rituals={ritualCards(content)}
      shopSlot={
        <HomeShopRail arm={shopArm} items={railItems} trackCtx={shopTrackCtx} />
      }
      onDownloadClick={onDownloadClick}
      onRailClick={onRailClick}
      onNavigate={(href, e) => {
        e.preventDefault();
        navigate(href);
      }}
      railRef={railsRef}
    />
  );
}

function useHomeView(sid: string, rails: number, live: boolean) {
  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("lp_home_viewed", { rails, live_rail: live }, { sid, story: STORY });
  }, [sid, rails, live]);
}

function useRailExposure(sid: string, content: HomeContent) {
  const fired = useRef<Set<string>>(new Set());
  const nodes = useRef<Map<string, HTMLElement>>(new Map());

  useEffect(() => {
    if (typeof IntersectionObserver === "undefined") return;
    const seen = fired.current;
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          const id = (entry.target as HTMLElement).dataset.rail;
          if (!id || seen.has(id)) continue;
          seen.add(id);
          track("lp_home_rail_viewed", { rail: id }, { sid, story: STORY });
          observer.unobserve(entry.target);
        }
      },
      { threshold: 0.4 },
    );
    for (const node of nodes.current.values()) observer.observe(node);
    return () => observer.disconnect();
  }, [sid, content]);

  return (id: string) => (node: HTMLElement | null) => {
    if (node) {
      node.dataset.rail = id;
      nodes.current.set(id, node);
    } else {
      nodes.current.delete(id);
    }
  };
}
