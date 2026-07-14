import { MkAssetNotFound } from "@ui/marketplace/pages/MkAssetDetailView";

import AssetDetailView from "@features/components/marketplace/AssetDetailView";
import UpstreamUnavailable from "@features/components/UpstreamUnavailable";

import {
  quoteCreditPrices,
  type PriceQuotes,
} from "@data/lib/catalyst/marketplace/credit-quotes";
import {
  fetchCatalogItem,
  fetchNft,
  formatMana,
  parseItemId,
  toAssetDetail,
  nftToAssetDetail,
  type AssetDetail,
} from "@data/lib/catalyst/marketplace/index";

function cheaperWei(a: string, b: string): boolean {
  try {
    return BigInt(a) < BigInt(b);
  } catch {
    return false;
  }
}
import {
  fetchOrders,
  toAssetListing,
  type AssetListing,
} from "@data/lib/catalyst/marketplace/orders";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { marketplaceMeta } from "@core/lib/seo/marketplace-meta";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/marketplace.$id";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const handle = { agentMarkdown: "assetDetail" } satisfies AgentMarkdownHandle;

export function meta({ loaderData }: Route.MetaArgs) {
  const nft = (loaderData as { nft?: { name?: string } } | undefined)?.nft;
  return marketplaceMeta(nft?.name || "Item");
}

const STORY: StoryId = "marketplace/asset";

const FALLBACK: Assignment = {
  variant: "control",
  flags: { prominentPrice: false },
  experimentKey: "marketplace_price_prominence",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const { id } = params;

  type Resolved = {
    nft: AssetDetail;
    kind: "collectible" | "land";
    onSale: boolean;
    rarity: string | null;
  } | null;

  const parsed = parseItemId(id);
  const signal = request.signal;

  const [resolvedRes, orders, story] = await Promise.all([
    (async (): Promise<{ value: Resolved; failed: boolean }> => {
      if (!parsed) return { value: null, failed: false };
      try {
        const item = await fetchCatalogItem(parsed.contractAddress, parsed.itemId, {
          signal,
        });
        if (item) {
          return {
            value: {
              nft: toAssetDetail(item),
              kind: "collectible",
              onSale: item.isOnSale,
              rarity: item.rarity ?? null,
            },
            failed: false,
          };
        }
        const result = await fetchNft(parsed.contractAddress, parsed.itemId, {
          signal,
        });
        if (result) {
          return {
            value: {
              nft: nftToAssetDetail(result),
              kind: "land",
              onSale: false,
              rarity: null,
            },
            failed: false,
          };
        }
        return { value: null, failed: false };
      } catch {
        return { value: null, failed: true };
      }
    })(),
    (async () => {
      if (!parsed) return null;
      try {
        return await fetchOrders(
          {
            contractAddress: parsed.contractAddress,
            itemId: `${parsed.contractAddress}-${parsed.itemId}`.toLowerCase(),
            status: "open",
            first: 24,
          },
          { signal },
        );
      } catch {
        return null;
      }
    })(),
    storyLoader(request, "marketplace/asset", FALLBACK),
  ]);

  const { sid, assignment, wrap } = story;
  const resolved = resolvedRes.value;
  const unavailable = resolvedRes.failed;

  const nft: AssetDetail | null = resolved?.nft ?? null;
  const onSale = resolved?.onSale ?? false;
  const rarity = resolved?.rarity ?? null;
  const kind = resolved?.kind ?? null;

  const listingOrders = parsed && nft && orders ? orders.data : [];
  if (nft && kind === "collectible" && !nft.order && listingOrders.length > 0) {
    const cheapest = listingOrders.reduce((a, b) =>
      cheaperWei(b.price, a.price) ? b : a,
    );
    const price = formatMana(cheapest.price);
    if (price) {
      nft.order = {
        price,
        credits: null,
        issuedId: 0,
        expiresLabel: "Listed on the marketplace",
        source: "listing",
      };
    }
  }
  const wantItemQuote = !!(parsed && nft && kind === "collectible" && nft.order);
  let listings: AssetListing[] = listingOrders.map((o) => toAssetListing(o));
  let quoteOk = false;
  if (wantItemQuote || listingOrders.length > 0) {
    const req = {
      items:
        wantItemQuote && parsed
          ? [{ itemId: parsed.itemId, collection: parsed.contractAddress }]
          : [],
      amounts: listingOrders.map((o) => o.price),
    };
    let quoted: PriceQuotes;
    try {
      quoted = await quoteCreditPrices(req, { signal });
      quoteOk = true;
    } catch {
      quoted = {
        items: req.items.map((r) => ({ ...r, credits: null })),
        amounts: req.amounts.map(() => null),
      };
    }
    if (wantItemQuote && nft?.order) {
      nft.order.credits = quoted.items[0]?.credits ?? null;
    }
    listings = listingOrders.map((o, i) =>
      toAssetListing(o, quoted.amounts[i] ?? null),
    );
  }

  track(
    "mk_asset_viewed",
    { item_id: id, rarity, on_sale: onSale, kind },
    {
      sid,
      story: STORY,
      variant: assignment.variant,
      experimentKey: assignment.experimentKey,
    },
  );
  const urlParams = new URL(request.url).searchParams;
  if (urlParams.get("src") === "ffw") {
    track(
      "ffw_shop_landing",
      { item_id: id, nid: urlParams.get("nid") ?? "", on_sale: onSale },
      { sid, story: STORY, experimentKey: "ffw_rules" },
    );
  }

  return wrap(
    {
      id,
      nft,
      listings,
      quoteOk,
      sid,
      assignment,
      unavailable,
    },
    { status: nft ? 200 : unavailable ? 503 : 404 },
  );
}

export default function MarketplaceAssetRoute({ loaderData }: Route.ComponentProps) {
  const { id, nft, listings, quoteOk, sid, assignment, unavailable } = loaderData;

  const prominentPrice = assignment.flags?.prominentPrice === true;

  if (!nft && unavailable) {
    return (
      <main className="mkasset-route mkasset-route--unavailable">
        <UpstreamUnavailable
          title="Marketplace is temporarily unavailable"
          message="We couldn't load this item right now. Please try again in a moment."
          backHref="/shop"
          backLabel="Back to the Shop"
        />
      </main>
    );
  }

  return nft ? (
    <AssetDetailView
      itemId={id}
      nft={nft}
      listings={listings}
      quoteOk={quoteOk}
      prominentPrice={prominentPrice}
      trackCtx={{
        sid,
        story: STORY,
        variant: assignment.variant,
        experimentKey: assignment.experimentKey,
      }}
    />
  ) : (
    <MkAssetNotFound />
  );
}
