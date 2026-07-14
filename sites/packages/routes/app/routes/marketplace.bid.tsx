import { useNavigate } from "react-router";

import MkFlowBanner from "@ui/marketplace/components/MkFlowBanner";

import FlowFallback from "@features/components/marketplace/FlowFallback";

import {
  fetchCatalogItem,
  parseItemId,
  toCardNetwork,
} from "@data/lib/catalyst/marketplace/index";
import { fetchOpenBids, type Bid } from "@data/lib/catalyst/marketplace/bid";
import { weiToManaOrNull } from "@data/lib/catalyst/marketplace/money";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import {
  hasWallet,
  getConnectedAddress,
  connectWallet,
  getChainId,
} from "@data/lib/auth/wallet";
import { signTypedData } from "@data/lib/auth/typed-data";
import { prepareBid } from "@data/lib/catalyst/marketplace/tx";
import BidWizard, { type BidAsset } from "@features/stories/marketplace/bid/BidWizard";
import type { ChainFn } from "@features/stories/marketplace/bid/machine";

import type { Route } from "./+types/marketplace.bid";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/bid";
const EXPERIMENT_KEY = "marketplace_bid_wizard";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: EXPERIMENT_KEY,
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const id = url.searchParams.get("id")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let asset: BidAsset | null = null;
  let openBids: Bid[] = [];
  let fallback = false;
  const parsed = id ? parseItemId(id) : null;
  if (parsed) {
    try {
      const signal = request.signal;
      const item = await fetchCatalogItem(parsed.contractAddress, parsed.itemId, {
        signal,
      });
      if (item) {
        asset = {
          id: item.id,
          name: item.name ?? "Untitled",
          category: item.category ?? "wearable",
          rarity: item.rarity ?? "common",
          network: toCardNetwork(item.network),
          floorMana: weiToManaOrNull(item.minListingPrice) ?? weiToManaOrNull(item.price),
          image: item.thumbnail ?? null,
        };
      }
      openBids = await fetchOpenBids(parsed.contractAddress, parsed.itemId, {
        signal,
      });
    } catch {
      fallback = true;
    }
  }
  if (!asset && id) fallback = true;

  const payload = {
    sid,
    step,
    asset,
    openBidCount: openBids.length,
    assignment,
    fallback,
  };
  return wrap(payload);
}

export default function MarketplaceBidRoute({ loaderData }: Route.ComponentProps) {
  const { sid, step, asset, assignment, fallback } = loaderData;
  const navigate = useNavigate();

  if (!asset) {
    return (
      <FlowFallback
        title={fallback ? "We couldn't load this item" : "Nothing to bid on yet"}
        subtitle={
          fallback
            ? "The item didn't load \u{2014} it may have been removed, or this is a temporary hiccup. Try again from its page."
            : "Offers start from an item page: open something you like and choose \u{201C}Make an offer\u{201D}."
        }
      />
    );
  }

  const item = asset;

  const realChain: ChainFn = async ({ phase, price, expiration }) => {
    const parsed = parseItemId(item.id);
    if (!parsed) throw new Error("This item can't be bid on (unrecognized id).");
    const chainId = item.network === "polygon" ? 137 : 1;

    if (phase === "approve") {
      if (!hasWallet())
        throw new Error(
          "No browser wallet found. Install MetaMask (or another EIP-1193 wallet).",
        );
      await (getConnectedAddress().then((a) => a ?? connectWallet()));
      const chain = await getChainId();
      if (chain !== chainId)
        throw new Error(
          `Switch your wallet to ${item.network} (chain ${chainId}) and retry.`,
        );
      return;
    }

    if (phase === "sign") {
      const from = (await getConnectedAddress()) ?? (await connectWallet());
      const priceMana = price ?? item.floorMana ?? 0;
      if (!priceMana || priceMana <= 0) throw new Error("Enter a bid amount first.");
      const { typedData } = prepareBid({
        chainId,
        bidder: from,
        tokenAddress: parsed.contractAddress,
        tokenId: parsed.itemId,
        priceMana,
        expiration,
      });
      await signTypedData(typedData, from);
      return;
    }

    return;
  };

  const exitBid = () => {
    const idx = (window.history.state as { idx?: number } | null)?.idx ?? 0;
    if (idx > 0) navigate(-1);
    else navigate(`/marketplace/${encodeURIComponent(item.id)}`);
  };

  return (
    <main className="marketplace-bid">
      <BidWizard
        asset={item}
        chain={realChain}
        banner={
          <MkFlowBanner>
            <strong>Test mode {"\u{2014}"} no real offer will be made.</strong> Offers
            can&apos;t be submitted on this marketplace yet. You can try the
            flow, and your wallet may ask for a signature, but no bid will be
            placed and nothing will be charged.
          </MkFlowBanner>
        }
        onExit={exitBid}
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        initialStep={step ?? undefined}
      />
    </main>
  );
}
