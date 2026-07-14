import React, { useEffect, useRef } from "react";
import { Link, useSearchParams } from "react-router";

import ChromeShell from "@ui/components/ChromeShell";
import DclTopBar from "@ui/web/frames/DclTopBar";
import NewShopTabs from "@ui/marketplace/new-shop/NewShopTabs";
import MkAccountPage from "@ui/marketplace/pages/MkAccountPage";
import MkAccountPage2 from "@ui/marketplace/pages/MkAccountPage2";
import MkAccountCollectionsSection from "@ui/marketplace/pages/MkAccountCollectionsSection";
import MkOnSaleOnRentAccountSections from "@ui/marketplace/pages/MkOnSaleOnRentAccountSections";
import "@ui/components/chromeshell.css";
import "@ui/components/dappfooter.css";
import "@ui/web/frames/dcltopbar.css";
import "@ui/marketplace/new-shop/newshoptabs.css";
import "@ui/marketplace/pages/mkaccount.css";
import "@ui/marketplace/pages/mkaccountpage.css";
import "@ui/marketplace/pages/mkaccountpage2.css";
import "@ui/marketplace/pages/mkaccountcollectionssection.css";
import "@ui/marketplace/pages/mkonsaleonrentaccountsections.css";

import {
  fetchOwnedWearables,
  fetchOwnedEmotes,
  fetchOwnedNames,
  fetchOwnerOrders,
  fetchCreatorCollections,
  fetchCollectionTiles,
  toOwnedCard,
  toOnSaleRow,
  formatMana,
  type OwnedItem,
  type OwnedName,
  type Order,
  type OwnedList,
} from "@data/lib/catalyst/marketplace/account";
import { useAuth } from "@data/lib/auth/context";
import AccountSocial from "@features/components/social/AccountSocial";
import { openSignIn } from "@features/components/auth/signin-store";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { marketplaceMeta } from "@core/lib/seo/marketplace-meta";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/marketplace.account";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => marketplaceMeta("My Account");

const STORY: StoryId = "marketplace/account";

const PER_SECTION = 24;

const TABS = ["overview", "on-sale", "on-rent", "collections", "bids", "social"] as const;
type Tab = (typeof TABS)[number];

function readTab(params: URLSearchParams): Tab {
  const raw = params.get("tab")?.trim() ?? "";
  return (TABS as readonly string[]).includes(raw) ? (raw as Tab) : "overview";
}

const EMPTY_ITEMS: OwnedList<OwnedItem> = { elements: [], total: 0 };
const EMPTY_NAMES: OwnedList<OwnedName> = { elements: [], total: 0 };
const EMPTY_ORDERS: OwnedList<Order> = { elements: [], total: 0 };

type CollectionCard = {
  contractAddress: string;
  name: string;
  size: number;
  isOnSale: boolean;
  tiles: string[];
};

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "marketplace_account",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const tab = readTab(url.searchParams);
  const address =
    url.searchParams.get("address")?.trim().toLowerCase() || readWallet(request) || "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const sig = request.signal;

  const [wearables, emotes, names, orders]: [
    OwnedList<OwnedItem>,
    OwnedList<OwnedItem>,
    OwnedList<OwnedName>,
    OwnedList<Order>,
  ] = address
    ? await Promise.all([
        fetchOwnedWearables(address, { first: PER_SECTION }, { signal: sig }).catch(
          () => EMPTY_ITEMS,
        ),
        fetchOwnedEmotes(address, { first: PER_SECTION }, { signal: sig }).catch(
          () => EMPTY_ITEMS,
        ),
        fetchOwnedNames(address, { first: PER_SECTION }, { signal: sig }).catch(
          () => EMPTY_NAMES,
        ),
        fetchOwnerOrders(address, { first: PER_SECTION }, { signal: sig }).catch(
          () => EMPTY_ORDERS,
        ),
      ])
    : [EMPTY_ITEMS, EMPTY_ITEMS, EMPTY_NAMES, EMPTY_ORDERS];

  let collectionCards: CollectionCard[] = [];
  let collectionsTotal = 0;
  if (address && tab === "collections") {
    const cols = await fetchCreatorCollections(
      address,
      { first: PER_SECTION, sortBy: "newest" },
      { signal: sig },
    ).catch(() => ({ elements: [], total: 0 }) as OwnedList<never>);
    collectionsTotal = cols.total;
    const tiles = await Promise.all(
      cols.elements.map((c) =>
        fetchCollectionTiles(c.contractAddress, { signal: sig }),
      ),
    );
    collectionCards = cols.elements.map((c, i) => ({
      contractAddress: c.contractAddress,
      name: c.name ?? "Untitled collection",
      size: c.size ?? 0,
      isOnSale: c.isOnSale,
      tiles: tiles[i] ?? [],
    }));
  }

  const source: "live" | "empty" = address ? "live" : "empty";

  const payload = {
    sid,
    tab,
    address,
    source,
    fallback: source === "empty",
    ownedCards: [...wearables.elements, ...emotes.elements].map(toOwnedCard),
    ownedTotal: wearables.total + emotes.total,
    names: names.elements.map((n) => ({ name: n.name, price: formatMana(n.price) })),
    namesTotal: names.total,
    onSaleRows: orders.elements.map(toOnSaleRow),
    onSaleTotal: orders.total,
    collections: collectionCards,
    collectionsTotal,
  };

  return wrap(payload);
}

type LoaderData = {
  sid: string;
  tab: Tab;
  address: string;
  source: "live" | "empty";
  fallback: boolean;
  ownedCards: ReturnType<typeof toOwnedCard>[];
  ownedTotal: number;
  names: { name: string; price: string | null }[];
  namesTotal: number;
  onSaleRows: ReturnType<typeof toOnSaleRow>[];
  onSaleTotal: number;
  collections: CollectionCard[];
  collectionsTotal: number;
};

export default function MarketplaceAccount({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return <AccountSurface d={d} />;
}

function AccountSurface({ d }: { d: LoaderData }) {
  const [, setSearchParams] = useSearchParams();
  const auth = useAuth();

  useEffect(() => {
    if (d.source !== "empty") return;
    const me = auth.address;
    if (!me) return;
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("address", me);
        return next;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [d.source, auth.address, setSearchParams]);

  useAccountViewed(d.sid, d);

  function selectTab(tab: Tab) {
    track("mk_account_tab_changed", { tab }, { sid: d.sid, story: STORY });
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (tab === "overview") next.delete("tab");
        else next.set("tab", tab);
        return next;
      },
      { preventScrollReset: true },
    );
  }

  function onAssetClick(itemId: string) {
    track("mk_account_asset_clicked", { item_id: itemId }, { sid: d.sid, story: STORY });
  }

  const owned = d.ownedCards.map((c) => ({
    name: c.name,
    collection: c.collection,
    price: c.price ?? undefined,
    rarity: c.rarity,
    network: c.network,
    image: c.image,
  }));

  const topbar = <DclTopBar variant="sites" active="shop" />;

  return (
    <ChromeShell className="mk" ariaLabel="My Assets" topbar={topbar} subnav={false}>
    <div className="mk-account">
      <h1 className="u-sr-only">My Account</h1>

      <div className="mk-account__subnav">
        <NewShopTabs tabs={SHOP_TABS} active="my-assets" />
      </div>

      <nav className="mk-account__sections" aria-label="Account sections">
        <NewShopTabs
          className="nstabs--sub"
          ariaLabel="Account sections"
          tabs={TABS.map((t) => ({ id: t, label: TAB_LABELS[t] }))}
          active={d.tab}
          onTab={(id) => selectTab(id as Tab)}
        />
      </nav>

      {d.source === "empty" ? (
        auth.address ? (
          <div className="mk-account__banner" role="status">
            Loading your account{"\u{2026}"}
          </div>
        ) : (
          <div className="mk-account__banner" role="status">
            <strong>Sign in to view your account.</strong>
            <span style={{ display: "block", marginTop: 4, color: "var(--lm-ink-3)" }}>
              See your items, NAMEs and listings.
            </span>
            <button type="button" onClick={() => openSignIn()} className="mk-account__signin">
              Sign in
            </button>
          </div>
        )
      ) : null}

      {d.tab === "overview" ? (
        <>
          <MkAccountPage chrome={false} owned={owned as React.ComponentProps<typeof MkAccountPage>["owned"]} initialSection="wearables" />
          {owned.length > 0 ? (
            <div className="mk-account__links" hidden aria-hidden="true">
              {d.ownedCards.map((c) => (
                <Link
                  key={c.id}
                  prefetch="intent"
                  to={`/marketplace/${encodeURIComponent(c.id)}`}
                  onClick={() => onAssetClick(c.id)}
                >
                  {c.name}
                </Link>
              ))}
            </div>
          ) : null}
        </>
      ) : null}

      {d.tab === "on-sale" ? (
        <MkOnSaleOnRentAccountSections chrome={false} type="sale" onSale={d.onSaleRows} onRent={[]} />
      ) : null}
      {d.tab === "on-rent" ? (
        <MkOnSaleOnRentAccountSections chrome={false} type="rent" onSale={d.onSaleRows} onRent={[]} isEmpty />
      ) : null}

      {d.tab === "collections" ? (
        <MkAccountCollectionsSection
          chrome={false}
          collections={d.collections}
          count={d.collectionsTotal}
        />
      ) : null}

      {d.tab === "social" ? <AccountSocial /> : null}

      {d.tab === "bids" ? (
        <MkAccountPage2
          chrome={false}
          account={{
            name: "My account",
            address: d.address,
            description:
              "Bids you have placed and received will appear here.",
            cover: null,
            links: [],
          }}
          items={[]}
          state="empty"
        />
      ) : null}
    </div>
    </ChromeShell>
  );
}

const TAB_LABELS: Record<Tab, string> = {
  overview: "Overview",
  "on-sale": "On Sale",
  "on-rent": "On Rent",
  collections: "Collections",
  bids: "Bids",
  social: "Social",
};

const SHOP_TABS = [
  { id: "overview", label: "Overview", href: "/shop" },
  { id: "all-assets", label: "All Assets", href: "/shop?tab=all-assets" },
  { id: "names", label: "NAMEs", href: "/marketplace/names" },
  { id: "my-assets", label: "My Assets", href: "/marketplace/account" },
  { id: "my-favorites", label: "My Favorites", href: "/shop?tab=my-favorites" },
  { id: "cart", label: "Cart", href: "/marketplace/cart" },
] as const;

function useAccountViewed(sid: string, d: LoaderData) {
  const last = useRef<string | null>(null);
  useEffect(() => {
    const key = `${d.source}`;
    if (last.current === key) return;
    last.current = key;
    track(
      "mk_account_viewed",
      {
        owned: d.ownedTotal,
        on_sale: d.onSaleTotal,
        on_rent: 0,
        names: d.namesTotal,
        source: d.source,
      },
      { sid, story: STORY },
    );
  }, [sid, d.source, d.ownedTotal, d.onSaleTotal, d.namesTotal]);
}
