import { useEffect, useRef } from "react";

import GvNotFound from "@ui/governance/pages/GvNotFound";

import {
  loadBidVoteContext,
  defaultBidId,
  type BidVoteContext,
} from "@data/lib/catalyst/governance/vote-bid";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track, trackExposure } from "@core/lib/telemetry/track";
import BidVotingFlow from "@features/stories/governance/vote-bid/BidVotingFlow";

import type { Route } from "./+types/governance.proposals_.$id.bid-vote";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/vote-bid";

const FALLBACK: Assignment = {
  variant: "gated",
  flags: { reckonGate: true },
  experimentKey: "gv_bid_vote_flow",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const rawId = params.id;
  const bidId = !rawId || rawId === "default" ? defaultBidId() : rawId;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    "governance/vote-bid",
    FALLBACK,
    { skipExposure: true },
  );

  let ctx: BidVoteContext | null = null;
  try {
    ctx = await loadBidVoteContext(bidId, { signal: request.signal });
  } catch {
    ctx = null;
  }

  if (ctx) {
    trackExposure({
      sid,
      story: STORY,
      variant: assignment.variant,
      experimentKey: assignment.experimentKey,
    });
  }

  const payload = { bidId, ctx, sid, assignment };

  return wrap(payload);
}

export default function GovernanceBidVoteRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const { bidId, ctx, sid, assignment } = d;

  const opened = useRef(false);
  useEffect(() => {
    if (opened.current || !ctx) return;
    opened.current = true;
    track(
      "gv_bid_vote_opened",
      {
        bid_id: bidId,
        tender_id: ctx.tender.id,
        bids: ctx.field.length,
        live: ctx.live,
      },
      { sid, story: STORY, variant: assignment.variant, experimentKey: assignment.experimentKey },
    );
  }, [bidId, ctx, sid, assignment.variant, assignment.experimentKey]);

  if (!ctx) {
    return (
      <main className="governance-bid-vote-route governance-bid-vote-route--notfound">
        <GvNotFound />
      </main>
    );
  }

  return (
    <main className="governance-bid-vote-route">
      <BidVotingFlow
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        bidId={bidId}
        field={ctx.field}
        maxErrors={ctx.maxErrorsBeforeRedirect}
        snapshotUrl="#snapshot"
        retryTimer="30s"
      />
    </main>
  );
}
