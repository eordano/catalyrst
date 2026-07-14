import FlowFallback from "@features/components/marketplace/FlowFallback";

import { readWallet } from "@data/lib/auth/wallet-cookie";
import { loadCancelListing } from "@data/lib/catalyst/marketplace/orders.server";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import CancelListingWizard from "@features/stories/marketplace/cancel-listing/CancelListingWizard";
import type { CancelOrder, Ownership } from "@features/stories/marketplace/cancel-listing/machine";

import type { Route } from "./+types/marketplace.cancel";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "marketplace/cancel-listing";
const EXPERIMENT_KEY = "marketplace_cancel_wizard";

function parseOwnership(url: URL): Ownership {
  const raw = url.searchParams.get("ownership")?.trim();
  return raw === "other" || raw === "none" ? raw : "self";
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: EXPERIMENT_KEY,
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const owner = url.searchParams.get("owner")?.trim() || readWallet(request) || undefined;
  const orderId = url.searchParams.get("order")?.trim() || undefined;
  const ownership = parseOwnership(url);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const { listing, source, reason, owner: resolvedOwner } = await loadCancelListing({
    owner,
    orderId,
    opts: { signal: request.signal },
  });

  const trackCtx = {
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  };

  track(
    "mk_cancel_listing_viewed",
    {
      order_id: listing?.orderId ?? null,
      price: listing?.price ?? null,
      has_listing: listing != null,
      source,
      ownership,
    },
    trackCtx,
  );

  const payload = {
    sid,
    step,
    listing,
    ownership,
    owner: resolvedOwner,
    source,
    reason: reason ?? null,
    assignment,
  };
  return wrap(payload);
}

export default function MarketplaceCancel({ loaderData }: Route.ComponentProps) {
  const { sid, step, listing, ownership, source, reason, assignment } = loaderData;

  if (!listing && source === "unavailable") {
    return (
      <FlowFallback
        title="We couldn't load your listings"
        subtitle={`The marketplace didn't answer, so we can't confirm what you have on sale. Your listing may still be live \u{2014} reload in a moment before assuming it ended.${reason ? ` (${reason})` : ""}`}
        primaryHref="/marketplace/account"
        primaryLabel="View your assets"
      />
    );
  }

  if (!listing) {
    return (
      <FlowFallback
        title="No active listing to cancel"
        subtitle="You have no open sale listings right now, or this one already ended."
        primaryHref="/marketplace/account"
        primaryLabel="View your assets"
      />
    );
  }

  const order: CancelOrder = {
    orderId: listing.orderId,
    owner: listing.owner,
    price: listing.price,
    name: listing.name,
    network: listing.network,
  };

  return (
    <main className="mkcancel-route">
      <CancelListingWizard
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        order={order}
        ownership={ownership}
        initialStep={step ?? undefined}
      />
    </main>
  );
}
