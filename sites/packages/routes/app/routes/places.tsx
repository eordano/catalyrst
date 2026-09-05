import { useEffect, useRef, useState } from "react";
import type { ShouldRevalidateFunctionArgs } from "react-router";
import { Link, useNavigation, useSearchParams } from "react-router";

import ExploreChrome from "@ui/explorer/frames/ExploreChrome";
import PlaceCard from "@ui/components/PlaceCard";
import SearchField from "@ui/atoms/SearchField";
import "@ui/explorer/pages/places.css";

import { mapPlace, type Place, type Category } from "@data/lib/catalyst/places/index";
import { loadPlaces, loadCategories } from "@data/lib/catalyst/places/index.server";
import {
  fetchCatalog,
  isCatalogItemBuyable,
  toCollectibleCard,
  type CollectibleCard,
} from "@data/lib/catalyst/marketplace/index";
import { makeSearchCache } from "@core/lib/router/client-cache";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import { experimentActive, resolveFlag } from "@core/lib/experiments/flags";
import { type Assignment } from "@core/lib/experiments/assign";
import { parseVariantOverride, storyLoader } from "@core/lib/experiments/story-loader";
import {
  PLACES_SHOP_ENTRY_ARMS,
  PLACES_SHOP_ENTRY_EXPERIMENT_KEY,
  activePlacesShopEntryExperiment,
  armOverride,
  shopEntryFromFlags,
  type PlacesShopEntryArm,
} from "@core/lib/experiments/places-shop-entry";
import { track, trackExposure, type TrackContext } from "@core/lib/telemetry/track";
import PlacesShopEntry from "@features/stories/misc/places-shop-entry/PlacesShopEntry";

import type { Route } from "./+types/places";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "misc/browse-places";
const SHOP_ENTRY_STORY: StoryId = "misc/places-shop-entry";

const PLACES_LIMIT = 60;
const RAIL_ITEMS = 4;

export const handle = { agentMarkdown: "placesIndex" } satisfies AgentMarkdownHandle;

const noop = () => {};

const FALLBACK: Assignment = {
  variant: "live-signals",
  flags: { showLiveCount: true, showLikeRate: true },
  experimentKey: "browse-places-live-signals",
};

const SHOP_ENTRY_FALLBACK: Assignment = {
  variant: "base",
  flags: { shopEntry: "base" },
  experimentKey: PLACES_SHOP_ENTRY_EXPERIMENT_KEY,
};

function forcedShopArm(url: URL): PlacesShopEntryArm | undefined {
  const raw =
    armOverride(url, PLACES_SHOP_ENTRY_ARMS.map((id) => ({ id }))) ??
    parseVariantOverride(url, PLACES_SHOP_ENTRY_EXPERIMENT_KEY);
  if (!raw) return undefined;
  return (PLACES_SHOP_ENTRY_ARMS as readonly string[]).includes(raw)
    ? (raw as PlacesShopEntryArm)
    : undefined;
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const category = url.searchParams.get("category")?.trim() ?? "";
  const search = url.searchParams.get("search")?.trim() ?? "";
  const rawOffset = Number.parseInt(url.searchParams.get("offset") ?? "0", 10);
  const offset = Number.isFinite(rawOffset) && rawOffset > 0 ? rawOffset : 0;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  // Second experiment on this surface, its own key, assigned independently.
  // Draft until activated: a non-empty flags-service override row (or the
  // PLACES_SHOP_ENTRY_EXPERIMENT env var) turns it on; otherwise every session
  // gets base. ?arm= / ?variant=places_shop_entry:<arm> still force a preview.
  const shop = await storyLoader(request, SHOP_ENTRY_STORY, SHOP_ENTRY_FALLBACK, {
    skipExposure: true,
  });
  const shopActive = await experimentActive(PLACES_SHOP_ENTRY_EXPERIMENT_KEY, {
    envActive:
      activePlacesShopEntryExperiment(
        typeof process !== "undefined"
          ? process.env?.PLACES_SHOP_ENTRY_EXPERIMENT
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
      experimentKey: PLACES_SHOP_ENTRY_EXPERIMENT_KEY,
    };
  }
  const shopArm: PlacesShopEntryArm =
    shopEntryFromFlags(shopAssignment.flags) ?? "base";

  const [placesRes, categories, layoutV2, railItems] = await Promise.all([
    loadPlaces({
      limit: PLACES_LIMIT,
      offset: offset || undefined,
      categories: category || undefined,
      search: search || undefined,
    }).catch(() => ({ data: [] as Place[], total: 0 })),
    loadCategories().catch(() => [] as Category[]),
    resolveFlag("places_layout_v2", false),
    shopArm === "rail"
      ? fetchCatalog({ first: 8, isOnSale: true, sortBy: "cheapest" })
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
    category,
    search,
    offset,
    places: placesRes.data,
    total: placesRes.total,
    categories,
    layoutV2,
    shopArm,
    shopVariant: shopAssignment.variant,
    shopExperimentKey: shopAssignment.experimentKey,
    railItems,
  });
}

type LoaderData = {
  sid: string;
  category: string;
  search: string;
  offset: number;
  places: Place[];
  total: number;
  categories: Category[];
  layoutV2: boolean;
  shopArm: PlacesShopEntryArm;
  shopVariant: string;
  shopExperimentKey: string;
  railItems: CollectibleCard[] | null;
};

const cache = makeSearchCache<LoaderData>();

export async function clientLoader({
  request,
  serverLoader,
}: Route.ClientLoaderArgs) {
  const key = cache.key(request);
  const hit = cache.get(key);
  if (hit) return hit;
  const fresh = (await serverLoader()) as LoaderData;
  cache.set(key, fresh);
  return fresh;
}

export function shouldRevalidate({
  currentUrl,
  nextUrl,
  defaultShouldRevalidate,
}: ShouldRevalidateFunctionArgs) {
  if (
    currentUrl.pathname === nextUrl.pathname &&
    currentUrl.search === nextUrl.search
  ) {
    return false;
  }
  return defaultShouldRevalidate;
}

export default function Places({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  return (
    <PlacesExplorer
      sid={d.sid}
      activeCategory={d.category}
      search={d.search}
      offset={d.offset}
      places={d.places}
      total={d.total}
      categories={d.categories}
      layoutV2={d.layoutV2}
      shopArm={d.shopArm}
      railItems={d.railItems}
      shopTrackCtx={{
        sid: d.sid,
        story: SHOP_ENTRY_STORY,
        variant: d.shopVariant,
        experimentKey: d.shopExperimentKey,
      }}
    />
  );
}

type ExplorerProps = {
  sid: string;
  activeCategory: string;
  search: string;
  offset: number;
  places: Place[];
  total: number;
  categories: Category[];
  layoutV2: boolean;
  shopArm: PlacesShopEntryArm;
  railItems: CollectibleCard[] | null;
  shopTrackCtx: TrackContext;
};

type Accumulated = { key: string; offset: number; rows: Place[] };

function accumulate(
  prev: Accumulated,
  key: string,
  offset: number,
  places: Place[],
): Accumulated {
  if (prev.key !== key || offset === 0) return { key, offset, rows: places };
  if (offset === prev.offset) return { key, offset, rows: places };
  if (offset > prev.offset) {
    return { key, offset, rows: [...prev.rows.slice(0, offset), ...places] };
  }
  return { key, offset, rows: prev.rows.slice(0, offset + places.length) };
}

function PlacesExplorer({
  sid,
  activeCategory,
  search,
  offset,
  places,
  total,
  categories,
  layoutV2,
  shopArm,
  railItems,
  shopTrackCtx,
}: ExplorerProps) {
  const [, setSearchParams] = useSearchParams();
  const navigation = useNavigation();

  const key = `${activeCategory}|${search}`;
  const [acc, setAcc] = useState<Accumulated>(() => ({
    key,
    offset,
    rows: places,
  }));

  useEffect(() => {
    setAcc((prev) => accumulate(prev, key, offset, places));
  }, [key, offset, places]);

  const rows = acc.key === key ? acc.rows : places;

  useExposure(sid, rows.length, activeCategory, search);

  const cards = rows.map(mapPlace);
  const canLoadMore = rows.length < total && places.length === PLACES_LIMIT;
  const loadingMore = navigation.state === "loading";

  function updateParam(paramKey: string, value: string) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (value) next.set(paramKey, value);
        else next.delete(paramKey);
        if (paramKey !== "offset") next.delete("offset");
        return next;
      },
      { preventScrollReset: true },
    );
  }

  function onCardClick(placeId: string) {
    track("place_card_clicked", { place_id: placeId }, { sid, story: STORY });
  }

  return (
    <ExploreChrome active="places" onTab={noop} onClose={noop}>
      <div className="pl" data-places-layout={layoutV2 ? "v2" : "v1"}>
        <div className="pl__head">
          <h1 className="pl__title">Places</h1>
          <div className="pl__actions">
            <div className="pl__search">
              <SearchField
                placeholder="Search"
                value={search}
                onChange={(v: string) => updateParam("search", v)}
              />
            </div>
          </div>
        </div>

        <PlacesShopEntry arm={shopArm} items={railItems} trackCtx={shopTrackCtx} />

        <div className="pl__cats" role="tablist" aria-label="Place categories">
          <CategoryPill
            label="All"
            active={activeCategory === ""}
            onSelect={() => updateParam("category", "")}
          />
          {categories.map((c) => (
            <CategoryPill
              key={c.name}
              label={c.i18n?.en ?? c.name}
              active={activeCategory === c.name}
              onSelect={() => updateParam("category", c.name)}
            />
          ))}
        </div>

        {cards.length > 0 ? (
          <div className="pl__grid">
            {cards.map((card, i) => (
              <Link
                key={card.id}
                to={`/places/${encodeURIComponent(card.id)}`}
                onClick={() => onCardClick(card.id)}
                aria-label={card.title}
                prefetch="intent"
                style={{ display: "block", textDecoration: "none", color: "inherit" }}
              >
                <PlaceCard
                  title={card.title}
                  image={card.image}
                  players={card.players}
                  rating={card.rating}
                  coords={card.coords}
                  live={card.live ? 1 : undefined}
                  featured={card.featured}
                  creator={card.creator}
                  hue={(i * 47) % 360}
                  to={undefined}
                />
              </Link>
            ))}
          </div>
        ) : (
          <p style={{ color: "rgba(255,255,255,0.7)", padding: "8px 0" }}>
            No places found.
          </p>
        )}

        {canLoadMore && (
          <div style={{ display: "flex", justifyContent: "center", padding: "24px 0" }}>
            <button
              type="button"
              className="pl__pill"
              disabled={loadingMore}
              onClick={() => updateParam("offset", String(rows.length))}
            >
              {loadingMore ? "Loading\u{2026}" : `Load more (${rows.length} of ${total})`}
            </button>
          </div>
        )}
      </div>
    </ExploreChrome>
  );
}

type CategoryPillProps = {
  label: string;
  active: boolean;
  onSelect: () => void;
};

function CategoryPill({ label, active, onSelect }: CategoryPillProps) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      className={"pl__pill" + (active ? " is-active" : "")}
      onClick={onSelect}
    >
      {label}
    </button>
  );
}

function useExposure(
  sid: string,
  count: number,
  category: string,
  search: string,
) {
  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${category}|${search}`;
    if (lastKey.current === key) return;
    lastKey.current = key;
    track(
      "place_list_viewed",
      { count, category: category || null, search: search || null },
      { sid, story: STORY },
    );
  }, [sid, count, category, search]);
}
