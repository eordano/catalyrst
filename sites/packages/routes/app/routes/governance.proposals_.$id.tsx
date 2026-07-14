import { Suspense, useEffect, useRef } from "react";
import { Await, Outlet, useMatches } from "react-router";

import GvProposalDetail from "@ui/governance/pages/GvProposalDetail";
import GvNotFound from "@ui/governance/pages/GvNotFound";

import {
  loadProposalDetail,
  fetchAuthorProfile,
  type ProposalDetailResult,
} from "@data/lib/catalyst/governance/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import {
  loadProposalVotes,
  loadProposalComments,
} from "@data/lib/catalyst/governance/proposal-extras";
import { track, trackExposure } from "@core/lib/telemetry/track";
import GovernanceVote from "@features/stories/governance/vote/GovernanceVote";

import type { Route } from "./+types/governance.proposals_.$id";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/vote";

export const handle = { agentMarkdown: "proposalDetail" } satisfies AgentMarkdownHandle;

const FALLBACK: Assignment = {
  variant: "control",
  flags: { guided: false },
  experimentKey: "gv_vote_flow",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const { id } = params;

  const [detail, votes, commentsData, story] = await Promise.all([
    loadProposalDetail(id, { signal: request.signal }).catch(
      () => undefined as ProposalDetailResult | null | undefined,
    ),
    loadProposalVotes(id, { signal: request.signal }),
    loadProposalComments(id, { signal: request.signal }),
    storyLoader(request, "governance/vote", FALLBACK, { skipExposure: true }),
  ]);
  const { sid, assignment, wrap } = story;
  const unavailable = detail === undefined;
  const resolved = detail ?? null;
  const proposal = resolved?.proposal ?? null;

  const authorName: Promise<string | null> = proposal
    ? (async () => {
        try {
          const addr = resolved?.authorAddress ?? null;
          if (!addr) return proposal.author ?? null;
          const prof = await fetchAuthorProfile(addr, {
            signal: request.signal,
          });
          return prof?.name || proposal.author || null;
        } catch {
          return proposal.author ?? null;
        }
      })()
    : Promise.resolve(null);

  if (proposal) {
    trackExposure({
      sid,
      story: STORY,
      variant: assignment.variant,
      experimentKey: assignment.experimentKey,
    });
  }

  return wrap(
    {
      id,
      proposal,
      votes,
      comments: commentsData.comments,
      commentsTotal: commentsData.total,
      sid,
      assignment,
      authorName,
      unavailable,
    },
    { status: proposal ? 200 : unavailable ? 503 : 404 },
  );
}

export default function GovernanceProposalDetailRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { id, proposal, votes, comments, commentsTotal, sid, assignment, unavailable } = d;

  const matches = useMatches();
  const hasChild =
    matches[matches.length - 1]?.id !== "routes/governance.proposals_.$id";

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || !proposal || hasChild) return;
    viewed.current = true;
    track(
      "gv_proposal_viewed",
      { proposal_id: id, category: proposal.type, status: proposal.status },
      { sid, story: STORY, variant: assignment.variant, experimentKey: assignment.experimentKey },
    );
  }, [id, proposal, hasChild, sid, assignment.variant, assignment.experimentKey]);

  if (hasChild) return <Outlet />;

  if (!proposal) {
    return (
      <main className="governance-detail-route governance-detail-route--notfound">
        {unavailable ? (
          <GvNotFound
            title="Proposal temporarily unavailable"
            description="We couldn't reach the governance service. Please try again in a moment."
          />
        ) : (
          <GvNotFound
            title="Proposal not found"
            description="The proposal you are looking for doesn't exist or has been removed."
          />
        )}
      </main>
    );
  }

  const active = proposal.status === "active";

  const detailWithAuthor = (author?: string | null) => (
    <GvProposalDetail
      proposal={
        { ...proposal, author: author ?? proposal.author } as React.ComponentProps<
          typeof GvProposalDetail
        >["proposal"]
      }
      state="ready"
      choices={votes?.choices ?? []}
      totalVotesLabel={votes?.totalVotesLabel}
      rationales={votes?.rationales ?? []}
      vpSeries={votes?.vpSeries}
      comments={comments}
      commentsTotal={commentsTotal}
    />
  );

  return (
    <main className="governance-detail-route">
      <Suspense fallback={detailWithAuthor()}>
        <Await resolve={d.authorName}>
          {(name) => detailWithAuthor(name as string | null)}
        </Await>
      </Suspense>

      {active && (
        <section
          aria-label="Cast your vote"
          style={{ maxWidth: 1180, margin: "0 auto", padding: "0 24px 56px" }}
        >
          <GovernanceVote
            proposalId={id}
            variant={assignment.variant}
            flags={assignment.flags}
            trackCtx={{
              sid,
              story: STORY,
              variant: assignment.variant,
              experimentKey: assignment.experimentKey,
            }}
            choices={["Yes", "No"]}
            totalVp={proposal.yourVp}
            snapshotUrl="#snapshot"
          />
        </section>
      )}
    </main>
  );
}
