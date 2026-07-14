import { useEffect, useRef } from "react";

import ChromeShell from "@ui/components/ChromeShell";
import DclTopBar from "@ui/web/frames/DclTopBar";
import NewShopTabs from "@ui/marketplace/new-shop/NewShopTabs";
import "@ui/components/chromeshell.css";
import "@ui/components/dappfooter.css";
import "@ui/web/frames/dcltopbar.css";
import "@ui/marketplace/new-shop/newshoptabs.css";

import { buildFeed, fetchSales, fetchTrades, ACTIVITY_TYPES, type ActivityType, type Sale, type Trade } from "@data/lib/catalyst/marketplace/activity";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import { MkActivityPage } from "@features/components/marketplace/MkActivityPage";

import type { Route } from "./+types/marketplace.activity";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/activity";
const PAGE_SIZE = 25;

const SHOP_TABS = [
  { id: "overview", label: "Overview", href: "/shop" },
  { id: "all-assets", label: "All Assets", href: "/shop?tab=all-assets" },
  { id: "names", label: "NAMEs", href: "/marketplace/names" },
  { id: "my-assets", label: "My Assets", href: "/marketplace/account" },
  { id: "my-favorites", label: "My Favorites", href: "/shop?tab=my-favorites" },
  { id: "cart", label: "Cart", href: "/marketplace/cart" },
] as const;

const SALE_TYPE_QUERY: Record<ActivityType, string | undefined> = {
  sale: undefined,
  listing: undefined,
  bid: "bid",
};

function readType(params: URLSearchParams): ActivityType | "" {
  const t = params.get("type")?.trim() ?? "";
  return (ACTIVITY_TYPES as readonly string[]).includes(t)
    ? (t as ActivityType)
    : "";
}

function readPage(params: URLSearchParams): number {
  const n = Number(params.get("page"));
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : 0;
}

const FALLBACK: Assignment = {
  variant: "default",
  flags: {},
  experimentKey: "marketplace_activity",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const type = readType(url.searchParams);
  const page = readPage(url.searchParams);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const skip = page * PAGE_SIZE;

  const wantSales = type !== "listing";
  const wantTrades = type !== "sale";

  const salesP = wantSales
    ? fetchSales(
        { first: PAGE_SIZE, skip, type: type ? SALE_TYPE_QUERY[type] : undefined },
        { signal: request.signal },
      )
        .then((r) => ({ ...r, ok: true }))
        .catch(() => ({ data: [] as Sale[], total: 0, ok: false }))
    : Promise.resolve({ data: [] as Sale[], total: 0, ok: true });

  const tradesP = wantTrades
    ? fetchTrades({ first: PAGE_SIZE, skip }, { signal: request.signal })
        .then((r) => ({ ...r, ok: true }))
        .catch(() => ({ data: [] as Trade[], total: 0, ok: false }))
    : Promise.resolve({ data: [] as Trade[], total: 0, ok: true });

  const [sales, trades] = await Promise.all([salesP, tradesP]);

  const fallback = !sales.ok || !trades.ok;

  const feedType = type === "" ? undefined : type;
  const entries = buildFeed(sales.data, trades.data, feedType);

  const sourceCount = Math.max(sales.data.length, trades.data.length);
  const hasNext = sourceCount >= PAGE_SIZE;

  const payload = {
    sid,
    type,
    page,
    pageSize: PAGE_SIZE,
    entries,
    hasNext,
    fallback,
    salesTotal: sales.total,
    tradesTotal: trades.total,
  };

  return wrap(payload);
}

export default function MarketplaceActivity({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  useActivityViewed(d.sid, d.entries.length, d.type, d.page);

  return (
    <ChromeShell
      className="mk"
      ariaLabel="Activity"
      topbar={<DclTopBar variant="sites" active="shop" />}
      subnav={false}
    >
      <NewShopTabs tabs={SHOP_TABS} />
      <MkActivityPage
        sid={d.sid}
        entries={d.entries}
        type={d.type}
        page={d.page}
        pageSize={d.pageSize}
        hasNext={d.hasNext}
        fallback={d.fallback}
        salesTotal={d.salesTotal}
        tradesTotal={d.tradesTotal}
      />
    </ChromeShell>
  );
}

function useActivityViewed(
  sid: string,
  count: number,
  type: ActivityType | "",
  page: number,
) {
  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${type}|${page}`;
    if (lastKey.current === key) return;
    lastKey.current = key;
    track(
      "mk_activity_viewed",
      { count, type: type || null, page },
      { sid, story: STORY },
    );
  }, [sid, count, type, page]);
}
