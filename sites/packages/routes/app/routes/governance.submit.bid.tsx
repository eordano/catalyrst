import { useMemo } from "react";

import { useAuth } from "@data/lib/auth/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { buildSubmitBid, getSubmitBidData } from "@data/lib/catalyst/governance/submit-bid";
import GvSubmitBidWizard from "@features/stories/governance/submit-bid/GvSubmitBidWizard";

import type { Route } from "./+types/governance.submit.bid";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-bid";

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_bid_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const linkedProposalId =
    url.searchParams.get("linked_proposal_id")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    DEFAULT_ASSIGNMENT,
  );

  const bidData = await getSubmitBidData({
    linkedProposalId,
    signal: request.signal,
  });

  const payload = { sid, step, assignment, bidData };
  return wrap(payload);
}

export default function GovernanceSubmitBid({ loaderData }: Route.ComponentProps) {
  const { sid, step, assignment, bidData } = loaderData;

  const { identity } = useAuth();
  const submitBid = useMemo(() => buildSubmitBid(identity), [identity]);

  return (
    <main className="governance-submit-bid">
      <GvSubmitBidWizard
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        data={bidData}
        initialStep={step ?? undefined}
        submitBid={submitBid}
      />
    </main>
  );
}
