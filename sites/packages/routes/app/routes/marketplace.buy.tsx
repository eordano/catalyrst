import MkBuyPage from "@ui/marketplace/pages/MkBuyPage";

import FlowFallback from "@features/components/marketplace/FlowFallback";

import { type BuyableListing } from "@data/lib/catalyst/marketplace/buy";
import { loadBuyListing } from "@data/lib/catalyst/marketplace/buy.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoaderWith } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import { withCreatorFunnel } from "@core/lib/telemetry/creator-funnel";
import BuyWizard from "@features/stories/marketplace/buy-nft/BuyWizard";
import { unavailablePurchase } from "@data/lib/catalyst/marketplace/unavailable-actions";

import type { Route } from "./+types/marketplace.buy";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/buy-nft";
const EXPERIMENT_KEY = "mk_buy_wizard";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: EXPERIMENT_KEY,
};

type Display = {
  name: string;
  rarity: string;
  category: string;
  kind: BuyableListing["asset"]["kind"];
  image: string | null;
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const item = url.searchParams.get("item")?.trim() || undefined;
  const nft = url.searchParams.get("nft")?.trim() || undefined;
  const {
    sid,
    assignment,
    wrap,
    data: { listing, source, reason, itemId },
  } = await storyLoaderWith(request, "marketplace/buy-nft", FALLBACK, () =>
    loadBuyListing({
      itemId: item,
      nftId: nft,
      opts: { signal: request.signal },
    }),
  );

  if (!listing) {
    const payload = {
      listing: null,
      display: null,
      source,
      reason: reason ?? null,
      itemId,
      sid,
      assignment,
    };
    return wrap(payload);
  }

  track(
    "mk_buy_viewed",
    {
      asset_id: listing.assetId,
      price_mana: listing.priceMana,
      network: listing.network,
      source,
    },
    {
      sid,
      story: STORY,
      variant: assignment.variant,
      experimentKey: assignment.experimentKey,
    },
  );

  const display: Display = {
    name: listing.asset.name,
    rarity: listing.asset.rarity,
    category: listing.asset.category,
    kind: listing.asset.kind,
    image: listing.asset.image ?? null,
  };

  const payload = { listing, display, source, reason: reason ?? null, itemId, sid, assignment };
  return wrap(payload);
}

export default function MarketplaceBuyRoute({ loaderData }: Route.ComponentProps) {
  const { listing, display, source, reason, sid, assignment } = loaderData as {
    listing: BuyableListing | null;
    display: Display | null;
    source: "catalyst" | "empty" | "unavailable";
    reason: string | null;
    itemId: string;
    sid: string;
    assignment: Assignment;
  };

  if (source === "unavailable") {
    return (
      <FlowFallback
        title="We couldn't load this listing"
        subtitle={`The marketplace didn't answer, so there is no price we can stand behind. This does not mean the item was sold \u{2014} reload in a moment to try again.${reason ? ` (${reason})` : ""}`}
      />
    );
  }

  if (!listing || !display) {
    return (
      <FlowFallback
        title="This listing isn't available"
        subtitle={"It may have just been sold or cancelled \u{2014} listings move fast. The item itself may still be on sale from another seller."}
      />
    );
  }


  return (
    <MkBuyPage found>
      <BuyWizard
        allowStepPreview={false}
        listing={{
          assetId: listing.assetId,
          contractAddress: listing.contractAddress,
          tokenId: listing.tokenId,
          priceMana: listing.priceMana,
          priceWei: listing.priceWei,
          network: listing.network,
          marketplaceAddress: listing.marketplaceAddress,
          chainId: listing.chainId,
          seller: listing.seller,
        }}
        connect={unavailablePurchase}
        approve={unavailablePurchase}
        commit={unavailablePurchase}
        display={display}
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        track={withCreatorFunnel(undefined, { sid })}
      />
    </MkBuyPage>
  );
}
