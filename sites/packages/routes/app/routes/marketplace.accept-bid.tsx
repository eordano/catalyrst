import FlowFallback from "@features/components/marketplace/FlowFallback";

import AcceptBidWizard from "@features/stories/marketplace/accept-bid/AcceptBidWizard";

import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Bid } from "@data/lib/catalyst/marketplace/bids";
import { loadReceivedBids, findBid } from "@data/lib/catalyst/marketplace/bids.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import type { Route } from "./+types/marketplace.accept-bid";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/accept-bid";
const EXPERIMENT_KEY = "mk_accept_bid_wizard";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: EXPERIMENT_KEY,
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    "marketplace/accept-bid",
    FALLBACK,
  );

  const owner = url.searchParams.get("owner")?.trim() || readWallet(request);
  const { bids, source, reason } = await loadReceivedBids(owner, {
    signal: request.signal,
  });
  const requestedBidId = url.searchParams.get("bid");
  const bid: Bid | null = findBid(bids, requestedBidId) ?? bids[0] ?? null;

  const payload = {
    bid,
    bidSource: source,
    bidReason: reason ?? null,
    initialStep: url.searchParams.get("step") ?? undefined,
    sid,
    assignment,
  };

  return wrap(payload);
}

export default function MarketplaceAcceptBidRoute({ loaderData }: Route.ComponentProps) {
  const { bid, bidSource, bidReason, initialStep, sid, assignment } = loaderData;

  if (!bid && bidSource === "unavailable") {
    return (
      <FlowFallback
        title="We couldn't load your bids"
        subtitle={`The marketplace didn't answer, so we can't tell you whether anyone has bid on your items. Don't read this as "no offers" \u{2014} reload in a moment to try again.${bidReason ? ` (${bidReason})` : ""}`}
        primaryHref="/marketplace/account"
        primaryLabel="View your assets"
      />
    );
  }

  if (!bid) {
    return (
      <FlowFallback
        title="No bids to accept"
        subtitle="Nobody has bid on your items yet. When a buyer makes an offer on something you own, it shows up here to review."
        primaryHref="/marketplace/account"
        primaryLabel="View your assets"
      />
    );
  }

  return (
    <main className="acceptbid-route">
      <AcceptBidWizard
        bid={bid}
        initialStep={initialStep}
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
      />
    </main>
  );
}
