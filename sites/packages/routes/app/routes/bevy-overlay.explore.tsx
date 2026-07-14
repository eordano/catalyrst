import type { CSSProperties } from "react";
import { useCallback, useEffect, useRef } from "react";
import type { ShouldRevalidateFunctionArgs } from "react-router";
import { Link, useNavigate, useSearchParams } from "react-router";

import ExploreChrome, { type TabId } from "@ui/explorer/frames/ExploreChrome";
import PlaceCard from "@ui/components/PlaceCard";
import CardGrid from "@ui/components/CardGrid";
import NewShopAssetCard from "@ui/marketplace/new-shop/NewShopAssetCard";
import Reel from "@ui/explorer/pages/Reel";
import "@ui/explorer/pages/places.css";

import {
  mapPlace,
  type Place,
  type PlaceCardProps,
} from "@data/lib/catalyst/places/index";
import { loadPlaces } from "@data/lib/catalyst/places/index.server";
import {
  fetchCatalog,
  isCatalogItemBuyable,
  toCollectibleCard,
  type CollectibleCard,
} from "@data/lib/catalyst/marketplace/index";
import {
  seasonsToShellVM,
  type CreditsHubVM,
} from "@data/lib/catalyst/marketplace/credits";
import { loadSeasons } from "@data/lib/catalyst/marketplace/credits.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import { sendBridge } from "@features/components/bevy-overlay/bridge";
import { collectibleToShopCard } from "@features/lib/marketplace/favorites";
import CreditsHub from "@features/components/marketplace/CreditsHub";

import type { Route } from "./+types/bevy-overlay.explore";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "client/explore-open";

const PLACES_LIMIT = 24;
const CATALOG_LIMIT = 12;
const PARCEL_SIZE = 16;

type ExploreTab = "places" | "marketplace" | "reel" | "credits";

const TABS: { id: ExploreTab; label: string }[] = [
  { id: "places", label: "Places" },
  { id: "marketplace", label: "Marketplace" },
  { id: "reel", label: "Camera Reel" },
  { id: "credits", label: "Credits" },
];

function parseTab(raw: string | null): ExploreTab {
  if (raw === "marketplace" || raw === "reel" || raw === "credits") return raw;
  return "places";
}

function isPanelOpen(raw: string | null): boolean {
  return raw === null || raw === "explore";
}

const FALLBACK: Assignment = {
  variant: "hud-explore",
  flags: { embedsPlaces: true },
  experimentKey: "client_explore_open",
};

type TabItems<T> = { items: T[]; failed: boolean };

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const tab = parseTab(url.searchParams.get("tab"));

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let places: TabItems<Place> = { items: [], failed: false };
  let collectibles: TabItems<CollectibleCard> = { items: [], failed: false };
  let credits: { hub: CreditsHubVM | null; failed: boolean } = {
    hub: null,
    failed: false,
  };

  if (tab === "places") {
    places = await loadPlaces({ limit: PLACES_LIMIT })
      .then((r) => ({ items: r.data, failed: false }))
      .catch(() => ({ items: [], failed: true }));
  } else if (tab === "marketplace") {
    collectibles = await fetchCatalog({ first: CATALOG_LIMIT, isOnSale: true })
      .then((r) => ({
        items: r.data
          .filter(isCatalogItemBuyable)
          .map((it) => toCollectibleCard(it)),
        failed: false,
      }))
      .catch(() => ({ items: [], failed: true }));
  } else if (tab === "credits") {
    // loadSeasons maps every failure to null and a live read always parses,
    // so null means the service never answered, not an empty season.
    const seasons = await loadSeasons(request.signal);
    credits = {
      hub: seasons ? seasonsToShellVM(seasons) : null,
      failed: seasons === null,
    };
  }

  return wrap({ sid, tab, assignment, places, collectibles, credits });
}

export function shouldRevalidate({
  currentUrl,
  nextUrl,
  defaultShouldRevalidate,
}: ShouldRevalidateFunctionArgs) {
  if (
    currentUrl.pathname === nextUrl.pathname &&
    parseTab(currentUrl.searchParams.get("tab")) ===
      parseTab(nextUrl.searchParams.get("tab"))
  ) {
    return false;
  }
  return defaultShouldRevalidate;
}

type LoaderData = {
  sid: string;
  tab: ExploreTab;
  assignment: Assignment;
  places: TabItems<Place>;
  collectibles: TabItems<CollectibleCard>;
  credits: { hub: CreditsHubVM | null; failed: boolean };
};

export default function BevyOverlayExplore({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return <ExplorePanel {...d} />;
}

const STATE_MSG_STYLE: CSSProperties = {
  color: "rgba(255,255,255,0.7)",
  padding: "8px 0",
};

const CARD_LINK_STYLE: CSSProperties = {
  display: "block",
  textDecoration: "none",
  color: "inherit",
};

function ExplorePanel({
  sid,
  tab,
  assignment,
  places,
  collectibles,
  credits,
}: LoaderData) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  const open = isPanelOpen(searchParams.get("panel"));

  const ctx = {
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  };

  const openedPlaces = useRef(false);
  useEffect(() => {
    if (openedPlaces.current || !open || tab !== "places") return;
    openedPlaces.current = true;
    track("cl_explore_opened", { place_count: places.items.length }, ctx);
  }, [open, tab, places.items.length]);

  const onTab = useCallback(
    (next: ExploreTab) => {
      if (next === tab) return;
      setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev);
          params.set("panel", "explore");
          params.set("tab", next);
          return params;
        },
        { preventScrollReset: true },
      );
    },
    [tab, setSearchParams],
  );

  const onChromeTab = useCallback(
    (id: TabId) => {
      if (id === "places") onTab("places");
      if (id === "gallery") onTab("reel");
    },
    [onTab],
  );

  const onClose = useCallback(() => {
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("panel", "closed");
        params.delete("tab");
        return params;
      },
      { preventScrollReset: true },
    );
  }, [setSearchParams]);

  if (!open) return null;

  function onCardClick(card: PlaceCardProps) {
    track("place_card_clicked", { place_id: card.id }, ctx);
    const [px, py] = card.coords
      .split(",")
      .map((v) => Number.parseInt(v.trim(), 10));
    if (Number.isFinite(px) && Number.isFinite(py)) {
      sendBridge("Teleport", {
        x: px * PARCEL_SIZE + 8,
        z: py * PARCEL_SIZE + 8,
      });
    }
  }

  const chromeActive: TabId | undefined =
    tab === "places" ? "places" : tab === "reel" ? "gallery" : undefined;

  const placeCards = places.items.map(mapPlace);
  const shopCards = collectibles.items.map(collectibleToShopCard);

  return (
    <ExploreChrome active={chromeActive} onTab={onChromeTab} onClose={onClose}>
      <div className="pl">
        <div className="pl__head">
          <h1 className="pl__title">Explore</h1>
          <div className="pl__sections" role="tablist" aria-label="Explore tabs">
            {TABS.map((t) => (
              <button
                key={t.id}
                type="button"
                role="tab"
                aria-selected={t.id === tab}
                className={"pl__sectab" + (t.id === tab ? " is-active" : "")}
                onClick={() => onTab(t.id)}
              >
                {t.label}
              </button>
            ))}
          </div>
        </div>

        {tab === "places" &&
          (placeCards.length > 0 ? (
            <div className="pl__grid">
              {placeCards.map((card, i) => (
                <Link
                  key={card.id}
                  to={`/places/${encodeURIComponent(card.id)}`}
                  onClick={() => onCardClick(card)}
                  aria-label={card.title}
                  prefetch="intent"
                  style={CARD_LINK_STYLE}
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
                    presentational
                  />
                </Link>
              ))}
            </div>
          ) : places.failed ? (
            <p role="alert" style={STATE_MSG_STYLE}>
              We couldn't load places. The service didn't answer &#x2014; this is not
              an empty list. Try again in a moment.
            </p>
          ) : (
            <p style={STATE_MSG_STYLE}>No places are listed right now.</p>
          ))}

        {tab === "marketplace" &&
          (shopCards.length > 0 ? (
            <CardGrid min={200} gap={16}>
              {shopCards.map((c) => (
                <NewShopAssetCard
                  key={c.id}
                  name={c.name}
                  meta={c.meta}
                  price={c.price}
                  unit={c.unit}
                  rarity={c.rarity}
                  network={c.network}
                  image={c.image}
                  onOpen={() =>
                    navigate(`/marketplace/${encodeURIComponent(c.id)}`)
                  }
                />
              ))}
            </CardGrid>
          ) : collectibles.failed ? (
            <p role="alert" style={STATE_MSG_STYLE}>
              We couldn't load the marketplace. The catalog didn't answer &#x2014;
              this is not an empty shop. Try again in a moment.
            </p>
          ) : (
            <p style={STATE_MSG_STYLE}>Nothing is on sale right now.</p>
          ))}

        {tab === "reel" && <Reel />}

        {tab === "credits" &&
          (credits.hub ? (
            <CreditsHub
              sid={sid}
              hub={credits.hub}
              onClaim={() => navigate("/marketplace/credits")}
              onClose={() => onTab("places")}
            />
          ) : credits.failed ? (
            <p role="alert" style={STATE_MSG_STYLE}>
              We couldn't load credits. The seasons service didn't answer &#x2014; try
              again in a moment.
            </p>
          ) : (
            <p style={STATE_MSG_STYLE}>
              No credits season is running right now.
            </p>
          ))}
      </div>
    </ExploreChrome>
  );
}
