import MkBuyPage from "@ui/marketplace/pages/MkBuyPage";

import FlowFallback from "@features/components/marketplace/FlowFallback";

import { type BuyableListing } from "@data/lib/catalyst/marketplace/buy";
import { loadBuyListing } from "@data/lib/catalyst/marketplace/buy.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import { withCreatorFunnel } from "@core/lib/telemetry/creator-funnel";
import BuyWizard from "@features/stories/marketplace/buy-nft/BuyWizard";
import type { SimFn } from "@features/stories/marketplace/buy-nft/machine";
import {
  hasWallet,
  getConnectedAddress,
  connectWallet,
  getChainId,
} from "@data/lib/auth/wallet";
import { signTypedData } from "@data/lib/auth/typed-data";
import { prepareBuyMetaTx } from "@data/lib/catalyst/marketplace/tx";

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

  const { sid, assignment, wrap } = await storyLoader(
    request,
    "marketplace/buy-nft",
    FALLBACK,
  );

  const item = url.searchParams.get("item")?.trim() || undefined;
  const nft = url.searchParams.get("nft")?.trim() || undefined;
  const { listing, source, reason, itemId } = await loadBuyListing({
    itemId: item,
    nftId: nft,
    opts: { signal: request.signal },
  });

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

  const realConnect: SimFn = async () => {
    if (!hasWallet())
      throw new Error("No browser wallet found. Install MetaMask (or another EIP-1193 wallet).");
    const from = (await getConnectedAddress()) ?? (await connectWallet());
    if (from && from.toLowerCase() === listing.seller.toLowerCase())
      throw new Error("This is your own listing \u{2014} cancel it from My Assets instead of buying it.");
    const chain = await getChainId();
    if (listing.chainId != null && chain !== listing.chainId)
      throw new Error(
        `Switch your wallet to ${listing.network} (chain ${listing.chainId}) and retry.`,
      );
    return { txHash: "" };
  };
  const realCommit: SimFn = async () => {
    if (!listing.marketplaceAddress || listing.chainId == null)
      throw new Error("Listing is missing on-chain data (marketplace address / chain).");
    const from = (await getConnectedAddress()) ?? (await connectWallet());
    if (from && from.toLowerCase() === listing.seller.toLowerCase())
      throw new Error("This is your own listing \u{2014} cancel it from My Assets instead of buying it.");
    const { typedData } = prepareBuyMetaTx({
      marketplaceAddress: listing.marketplaceAddress,
      contractAddress: listing.contractAddress,
      tokenId: listing.tokenId,
      priceWei: listing.priceWei,
      chainId: listing.chainId,
      from,
    });
    await signTypedData(typedData, from);
    return { txHash: "" };
  };

  return (
    <MkBuyPage found>
      <BuyWizard
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
        connect={realConnect}
        commit={realCommit}
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
