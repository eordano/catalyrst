import { useState } from "react";
import { Link, useNavigate, useNavigation, useSearchParams } from "react-router";
import { href, searchHref } from "@core/lib/router/routes";

import ChromeShell from "@ui/components/ChromeShell";
import DclTopBar from "@ui/web/frames/DclTopBar";
import NewShopHome from "@ui/marketplace/new-shop/NewShopHome";
import NewShopBrowse from "@ui/marketplace/new-shop/NewShopBrowse";
import NewShopTabs, { type NewShopTab } from "@ui/marketplace/new-shop/NewShopTabs";
import NewShopAssetCard from "@ui/marketplace/new-shop/NewShopAssetCard";
import CardGrid from "@ui/components/CardGrid";
import type { ShopCard } from "@ui/marketplace/new-shop/NewShopHome";
import type { FilterGroup } from "@ui/marketplace/new-shop/NewShopFilterSidebar";
import type { RankRow } from "@ui/marketplace/new-shop/NewShopRankTable";

import "@ui/components/chromeshell.css";
import "@ui/components/dappfooter.css";
import "@ui/web/frames/dcltopbar.css";
import "@ui/atoms/toggle.css";
import "@ui/atoms/checkbox.css";
import "@ui/components/dropdown.css";
import "@ui/components/cardgrid.css";
import "@ui/marketplace/new-shop/newshoptabs.css";
import "@ui/marketplace/new-shop/newshopherobanner.css";
import "@ui/marketplace/new-shop/newshopassetcard.css";
import "@ui/marketplace/new-shop/newshopfeaturedrow.css";
import "@ui/marketplace/new-shop/newshopfiltersidebar.css";
import "@ui/marketplace/new-shop/newshopranktable.css";
import "@ui/marketplace/new-shop/newshophome.css";
import "@ui/marketplace/new-shop/newshopbrowse.css";

import { openSignIn } from "@features/components/auth/signin-store";
import { useAuth } from "@data/lib/auth/index";
import { getIdentity } from "@data/lib/auth/session";
import { parseItemRef } from "@data/lib/catalyst/marketplace/cart";
import { tryQuoteCreditItems } from "@data/lib/catalyst/marketplace/credit-quotes";
import {
  fetchCatalog,
  isCatalogItemBuyable,
  parseItemId,
  toCollectibleCard,
  type CatalogItem,
  type CollectibleCard,
} from "@data/lib/catalyst/marketplace/index";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { collectibleToShopCard } from "@features/lib/marketplace/favorites";
import { useFavorites } from "@features/lib/marketplace/use-favorites";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/shop";

const STORY = "marketplace/shop";
const CATALOG_LIMIT = 40;

type ShopTabId = "overview" | "all-assets" | "names" | "my-assets" | "my-favorites" | "cart";

const TABS: readonly NewShopTab<ShopTabId>[] = [
  { id: "overview", label: "Overview" },
  { id: "all-assets", label: "All Assets" },
  { id: "names", label: "NAMEs", href: "/marketplace/names" },
  { id: "my-assets", label: "My Assets" },
  { id: "my-favorites", label: "My Favorites" },
  { id: "cart", label: "Cart", href: "/marketplace/cart" },
];

const CATEGORY_OPTIONS = [
  { id: "wearable", label: "Wearables" },
  { id: "emote", label: "Emotes" },
];

const RARITIES = [
  "common", "uncommon", "rare", "epic", "legendary", "mythic", "unique", "exotic",
];

const SORT: { id: string; label: string }[] = [
  { id: "recently_listed", label: "Recently listed" },
  { id: "suggested", label: "Suggested" },
  { id: "cheapest", label: "Price: Low to High" },
  { id: "most_expensive", label: "Price: High to Low" },
  { id: "recently_sold", label: "Recently sold" },
];
const SORT_LABELS = SORT.map((s) => s.label);

type Filters = {
  tab: string;
  category: string;
  rarity: string;
  sortBy: string;
  search: string;
  page: number;
};

function readFilters(params: URLSearchParams): Filters {
  const page = Number.parseInt(params.get("page") ?? "0", 10);
  return {
    tab: params.get("tab")?.trim() || "overview",
    category: params.get("category")?.trim() ?? "",
    rarity: params.get("rarity")?.trim() ?? "",
    sortBy: params.get("sortBy")?.trim() || "recently_listed",
    search: params.get("search")?.trim() ?? "",
    page: Number.isFinite(page) && page > 0 ? page : 0,
  };
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const f = readFilters(url.searchParams);
  const { sid, wrap } = sidLoader(request);

  let items: CatalogItem[] = [];
  let total = 0;
  let fallback = false;
  let top: CatalogItem[] = [];
  let trending: CatalogItem[] = [];
  const isOverview = f.tab === "overview";
  const none = Promise.resolve({ data: [] as CatalogItem[], total: 0 });
  try {
    const [result, topResult, trendingResult] = await Promise.all([
      fetchCatalog({
        first: CATALOG_LIMIT,
        skip: f.page * CATALOG_LIMIT,
        category: f.category || undefined,
        rarity: f.rarity || undefined,
        isOnSale: true,
        sortBy: f.sortBy || undefined,
        search: f.search || undefined,
      }),
      isOverview ? fetchCatalog({ first: 6, isOnSale: true, sortBy: "most_expensive" }) : none,
      isOverview ? fetchCatalog({ first: 8, isOnSale: true, sortBy: "cheapest" }) : none,
    ]);
    items = result.data.filter(isCatalogItemBuyable);
    total = result.total;
    top = topResult.data.filter(isCatalogItemBuyable);
    trending = trendingResult.data.filter(isCatalogItemBuyable);
  } catch {
    fallback = true;
  }

  const uniq = new Map<string, CatalogItem>();
  for (const it of [...items, ...top, ...trending]) {
    if (!uniq.has(it.id)) uniq.set(it.id, it);
  }
  const quotables = [...uniq.values()];
  const credits = await tryQuoteCreditItems(
    quotables.map((it) => {
      const ref = parseItemId(it.id);
      return ref ? { itemId: ref.itemId, collection: ref.contractAddress } : null;
    }),
  );
  const creditsById = new Map<string, string | null>();
  quotables.forEach((it, i) => creditsById.set(it.id, credits[i] ?? null));
  const withCredits = (it: CatalogItem) =>
    toCollectibleCard(it, creditsById.get(it.id) ?? null);

  const cards = items.map(withCredits);
  const topCards = top.map(withCredits);
  const trendingCards = trending.map(withCredits);
  const payload = { sid, filters: f, cards, topCards, trendingCards, total, fallback };
  return wrap(payload);
}

const toShopCard = (c: CollectibleCard): ShopCard => collectibleToShopCard(c);

const toRankRow = (c: CollectibleCard): RankRow => ({
  id: c.id,
  name: c.name,
  image: c.image,
  floor: c.credits ?? c.price ?? undefined,
  unit: c.credits != null ? "credits" : "mana",
  network: c.network,
});

export default function MarketplaceShop({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const [, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const navigation = useNavigation();
  const loading = navigation.state === "loading";
  const auth = useAuth();
  const [buyError, setBuyError] = useState<string | null>(null);

  const f = d.filters;
  const shopCards = d.cards.map(toShopCard);
  const trendingShopCards = d.trendingCards.map(toShopCard);

  const { favCards, favIds, toggle, syncState } = useFavorites({ sid: d.sid, story: STORY });

  const heartable = new Map<string, ShopCard>();
  for (const c of [...shopCards, ...trendingShopCards, ...favCards]) {
    if (!heartable.has(c.id)) heartable.set(c.id, c);
  }

  function setMany(entries: Record<string, string>) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        for (const [key, value] of Object.entries(entries)) {
          if (value) next.set(key, value);
          else next.delete(key);
        }
        return next;
      },
      { preventScrollReset: true },
    );
  }
  const setParam = (key: string, value: string) => setMany({ [key]: value });

  function onTab(id: string) {
    track("mk_shop_tab", { tab: id }, { sid: d.sid, story: STORY });
    setParam("tab", id === "overview" ? "" : id);
  }

  function onOpenAsset(id: string) {
    track("mk_shop_card_clicked", { item_id: id }, { sid: d.sid, story: STORY });
    navigate(`/marketplace/${encodeURIComponent(id)}`);
  }

  function onBuyAsset(id: string) {
    track("mk_shop_buy_now", { item_id: id }, { sid: d.sid, story: STORY });
    const ref = parseItemRef(id);
    if (!ref) {
      setBuyError(null);
      navigate(`/marketplace/${encodeURIComponent(id)}`);
      return;
    }
    setBuyError(null);
    const dest = `/marketplace/checkout?express=${encodeURIComponent(id)}`;
    if (!getIdentity()) {
      openSignIn({ redirectTo: dest });
      return;
    }
    navigate(dest);
  }

  function onToggleFavorite(id: string) {
    toggle(heartable.get(id) ?? { id, name: id });
  }

  const topbar = <DclTopBar variant="sites" active="shop" />;

  return (
    <ChromeShell
      className="mk"
      ariaLabel="Shop"
      topbar={topbar}
      subnav={false}
    >
      {buyError ? (
        <p role="alert" style={{ color: "#ff7a7a", margin: "12px 24px 0", maxWidth: 1200, marginInline: "auto" }}>
          {buyError}
        </p>
      ) : null}

      {f.tab === "all-assets" ? (
        <BrowseTab
          filters={f}
          cards={shopCards}
          total={d.total}
          fallback={d.fallback}
          onTab={onTab}
          setParam={setParam}
          setMany={setMany}
          sid={d.sid}
          favIds={favIds}
          loading={loading}
          onOpenAsset={onOpenAsset}
          onBuyAsset={onBuyAsset}
          onToggleFavorite={onToggleFavorite}
        />
      ) : f.tab === "my-favorites" ? (
        <FavoritesTab
          cards={favCards}
          favIds={favIds}
          synced={syncState === "synced"}
          onTab={onTab}
          onOpenAsset={onOpenAsset}
          onBuyAsset={onBuyAsset}
          onToggleFavorite={onToggleFavorite}
        />
      ) : f.tab === "my-assets" ? (
        <AccountTab tab={f.tab} onTab={onTab} signedIn={!!auth.isConnected} />
      ) : (
        <OverviewTab
          cards={shopCards}
          trending={trendingShopCards}
          rankRows={d.topCards.map(toRankRow)}
          fallback={d.fallback}
          favIds={favIds}
          onTab={onTab}
          onOpenAsset={onOpenAsset}
          onBuyAsset={onBuyAsset}
          onToggleFavorite={onToggleFavorite}
          onBannerCta={(cat) => setMany({ tab: "all-assets", category: cat })}
        />
      )}
    </ChromeShell>
  );
}

function OverviewTab({
  cards,
  trending,
  rankRows,
  fallback,
  favIds,
  onTab,
  onOpenAsset,
  onBuyAsset,
  onToggleFavorite,
  onBannerCta,
}: {
  cards: ShopCard[];
  trending: ShopCard[];
  rankRows: RankRow[];
  fallback: boolean;
  favIds: string[];
  onTab: (id: string) => void;
  onOpenAsset: (id: string) => void;
  onBuyAsset: (id: string) => void;
  onToggleFavorite: (id: string, next: boolean) => void;
  onBannerCta: (category: string) => void;
}) {
  const featured = [
    { id: "deals", title: "Best Deals", cards: trending.slice(0, 8) },
    { id: "new", title: "New Listings", cards: cards.slice(0, 8) },
  ].filter((s) => s.cards.length > 0);

  const banners = [
    {
      id: "emotes",
      eyebrow: "Trending now",
      title: "Best Rated Emotes",
      subtitle: "The community's top-voted moves this week.",
      cta: "Shop emotes",
      tone: "purple" as const,
    },
    {
      id: "outfits",
      eyebrow: "Curated",
      title: "Week Selected Outfits",
      subtitle: "Hand-picked wearables to refresh your look.",
      cta: "Shop outfits",
      tone: "magenta" as const,
    },
  ];

  return (
    <>
      <NewShopHome
        tabs={TABS}
        activeTab="overview"
        onTab={onTab}
        banners={banners}
        featured={featured}
        favorites={favIds}
        onOpenAsset={onOpenAsset}
        onBuyAsset={onBuyAsset}
        onToggleFavorite={onToggleFavorite}
        onBannerCta={(id) => onBannerCta(id === "emotes" ? "emote" : "wearable")}
        onViewAll={() => onTab("all-assets")}
        rankTitle="Most Valuable"
        rankRows={rankRows}
        onOpenRank={onOpenAsset}
      />
      {fallback ? (
        <p className="mk" style={FALLBACK_STYLE} role="alert">
          Couldn&apos;t load the catalog right now. Please try again.
        </p>
      ) : null}
    </>
  );
}

function BrowseTab({
  filters,
  cards,
  total,
  fallback,
  onTab,
  setParam,
  setMany,
  sid,
  favIds,
  loading,
  onOpenAsset,
  onBuyAsset,
  onToggleFavorite,
}: {
  filters: Filters;
  cards: ShopCard[];
  total: number;
  fallback: boolean;
  onTab: (id: string) => void;
  setParam: (key: string, value: string) => void;
  setMany: (entries: Record<string, string>) => void;
  sid: string;
  favIds: string[];
  loading: boolean;
  onOpenAsset: (id: string) => void;
  onBuyAsset: (id: string) => void;
  onToggleFavorite: (id: string, next: boolean) => void;
}) {
  const groups: FilterGroup[] = [
    {
      id: "category",
      label: "Category",
      options: CATEGORY_OPTIONS.map((o) => ({
        id: o.id,
        label: o.label,
        checked: filters.category === o.id,
      })),
    },
    {
      id: "rarity",
      label: "Rarity",
      options: RARITIES.map((r) => ({
        id: r,
        label: r[0].toUpperCase() + r.slice(1),
        checked: filters.rarity === r,
      })),
    },
  ];

  const sortValue = SORT.find((s) => s.id === filters.sortBy)?.label ?? SORT_LABELS[0];

  function onOptionChange(groupId: string, optionId: string, checked: boolean) {
    const key = groupId === "rarity" ? "rarity" : "category";
    track("mk_shop_filter", { filter: key, value: checked ? optionId : null }, { sid, story: STORY });
    setMany({ [key]: checked ? optionId : "", page: "" });
  }

  function onSort(label: string) {
    const id = SORT.find((s) => s.label === label)?.id ?? "recently_listed";
    track("mk_shop_sort", { sort_by: id }, { sid, story: STORY });
    setMany({ sortBy: id === "recently_listed" ? "" : id, page: "" });
  }

  const totalPages = Math.max(1, Math.ceil(total / CATALOG_LIMIT));

  return (
    <>
      <NewShopBrowse
        tabs={TABS}
        activeTab="all-assets"
        onTab={onTab}
        groups={groups}
        cards={cards}
        itemCount={total.toLocaleString() + " items"}
        sortOptions={SORT_LABELS}
        sortValue={sortValue}
        onSort={onSort}
        favorites={favIds}
        onOptionChange={onOptionChange}
        searchValue={filters.search}
        onSearch={(q) => {
          track("mk_shop_search", { has_query: !!q }, { sid, story: STORY });
          setMany({ search: q, page: "" });
        }}
        filtersActive={!!(filters.category || filters.rarity || filters.search)}
        onClearFilters={() => {
          track("mk_shop_clear_filters", {}, { sid, story: STORY });
          setMany({ category: "", rarity: "", search: "", page: "" });
        }}
        page={filters.page}
        totalPages={totalPages}
        loading={loading}
        onPageChange={(p) => {
          track("mk_shop_page", { page: p }, { sid, story: STORY });
          setParam("page", p > 0 ? String(p) : "");
        }}
        emptyLabel={
          fallback
            ? "Couldn't load the catalog right now. Please try again."
            : "No items match your filters."
        }
        onOpenAsset={onOpenAsset}
        onBuyAsset={onBuyAsset}
        onToggleFavorite={onToggleFavorite}
      />
    </>
  );
}

function FavoritesTab({
  cards,
  favIds,
  synced,
  onTab,
  onOpenAsset,
  onBuyAsset,
  onToggleFavorite,
}: {
  cards: ShopCard[];
  favIds: string[];
  synced: boolean;
  onTab: (id: string) => void;
  onOpenAsset: (id: string) => void;
  onBuyAsset: (id: string) => void;
  onToggleFavorite: (id: string, next: boolean) => void;
}) {
  const favSet = new Set(favIds);
  return (
    <div className="mk" style={{ background: "var(--lm-bg)", minHeight: "100%" }}>
      <NewShopTabs tabs={TABS} active="my-favorites" onTab={onTab} />
      <div style={{ maxWidth: 1240, margin: "0 auto", padding: 24 }}>
        <p style={{ fontSize: 13, opacity: 0.7, margin: "0 0 16px" }} role="note">
          {synced
            ? "Favorites are saved on this device and synced to your account."
            : "Favorites are saved on this device only."}
        </p>
        {cards.length ? (
          <CardGrid min={200} gap={16}>
            {cards.map((c) => (
              <NewShopAssetCard
                key={c.id}
                name={c.name}
                meta={c.meta}
                price={c.price}
                unit={c.unit}
                rarity={c.rarity}
                network={c.network}
                image={c.image}
                favorited={favSet.has(c.id)}
                onToggleFavorite={() => onToggleFavorite(c.id, !favSet.has(c.id))}
                onOpen={() => onOpenAsset(c.id)}
                onBuy={() => onBuyAsset(c.id)}
              />
            ))}
          </CardGrid>
        ) : (
          <div style={PLACEHOLDER_STYLE}>
            <p style={{ fontSize: 15, fontWeight: 600, marginBottom: 12 }}>
              No favourites yet {"\u{2014}"} tap the heart on any item to save it here.
            </p>
            <Link to={searchHref("/shop", { tab: "all-assets" })} style={LINK_STYLE}>
              Browse all assets
            </Link>
          </div>
        )}
      </div>
    </div>
  );
}

function AccountTab({ tab, onTab, signedIn }: { tab: string; onTab: (id: string) => void; signedIn: boolean }) {
  return (
    <div className="mk" style={{ background: "var(--lm-bg)", minHeight: "100%" }}>
      <div style={{ maxWidth: 1200, margin: "0 auto", padding: "0 24px" }}>
        <div style={TABBAR_STYLE}>
          {TABS.map((t) =>
            t.href ? (
              <Link
                key={t.id}
                to={t.href}
                style={{ ...TAB_STYLE, ...(t.id === tab ? TAB_ACTIVE_STYLE : {}) }}
              >
                {t.label}
              </Link>
            ) : (
              <button
                key={t.id}
                type="button"
                onClick={() => onTab(t.id)}
                style={{ ...TAB_STYLE, ...(t.id === tab ? TAB_ACTIVE_STYLE : {}) }}
              >
                {t.label}
              </button>
            ),
          )}
        </div>
        <div style={PLACEHOLDER_STYLE}>
          <p style={{ fontSize: 15, fontWeight: 600, marginBottom: 12 }}>
            {signedIn
              ? "View everything you own on your account."
              : "Sign in to see the assets you own."}
          </p>
          <Link to={href("/marketplace/account")} prefetch="intent" style={LINK_STYLE}>
            Go to your account
          </Link>
        </div>
      </div>
    </div>
  );
}

const FALLBACK_STYLE: React.CSSProperties = {
  maxWidth: 1200,
  margin: "0 auto",
  padding: "0 24px 32px",
  color: "#ff7a7a",
};
const TABBAR_STYLE: React.CSSProperties = {
  display: "flex",
  gap: 4,
  padding: "0 4px",
  borderBottom: "1px solid var(--lm-line)",
};
const TAB_STYLE: React.CSSProperties = {
  appearance: "none",
  border: 0,
  background: "transparent",
  color: "var(--lm-ink-3)",
  font: "inherit",
  fontSize: 13,
  fontWeight: 700,
  letterSpacing: "0.4px",
  textTransform: "uppercase",
  padding: "14px 16px",
  cursor: "pointer",
};
const TAB_ACTIVE_STYLE: React.CSSProperties = {
  color: "var(--lm-ink)",
  boxShadow: "inset 0 -3px 0 var(--brand)",
};
const PLACEHOLDER_STYLE: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "center",
  textAlign: "center",
  padding: "80px 24px",
  color: "var(--lm-ink-2)",
};
const LINK_STYLE: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  padding: "10px 20px",
  borderRadius: 999,
  background: "var(--brand)",
  color: "#fff",
  textDecoration: "none",
  fontWeight: 700,
  fontSize: 13,
};
