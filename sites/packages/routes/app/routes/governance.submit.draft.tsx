import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import { useAuth } from "@data/lib/auth/index";
import { loadLinkedPolls, buildDraftData, VP_THRESHOLD, type DraftSubmitData } from "@data/lib/catalyst/governance/submit-draft";
import { shortAddress } from "@data/lib/catalyst/format/address";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GovernanceNotice from "@features/components/governance/GovernanceNotice";
import SubmitDraftWizard from "@features/components/governance/SubmitDraftWizard";

import type { Route } from "./+types/governance.submit.draft";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-draft";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_draft_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const { ok: pollsOk, polls } = await loadLinkedPolls({
    signal: request.signal,
  });

  const connectedOverride = url.searchParams.get("connected") === "0";
  const vpOverride = url.searchParams.get("vp") === "0";

  const payload = {
    sid,
    polls,
    pollsOk,
    connectedOverride,
    vpOverride,
    assignment,
  };

  return wrap(payload);
}

export default function GovernanceSubmitDraftRoute({
  loaderData,
}: Route.ComponentProps) {
  const { sid, polls, pollsOk, connectedOverride, vpOverride, assignment } =
    loaderData;

  const auth = useAuth();
  const connected = !connectedOverride && auth.isConnected;
  const address = connected && auth.address ? auth.address : "";
  const short = address ? shortAddress(address) : "Guest";
  const vp = vpOverride ? 0 : VP_THRESHOLD;

  const draftData: DraftSubmitData = buildDraftData({
    polls,
    pollsOk,
    account: { address, short, vp },
  });

  return (
    <main className="governance-submit-draft-route">
      <nav className="governance-submit-draft-route__crumbs" aria-label="Breadcrumb">
        <Link prefetch="intent" to={href("/governance")}>
          Governance
        </Link>
        <span aria-hidden="true"> / </span>
        <Link prefetch="intent" to={href("/governance/proposals")}>
          Proposals
        </Link>
        <span aria-hidden="true"> / </span>
        <span aria-current="page">Submit Draft</span>
      </nav>

      {!pollsOk && (
        <p
          className="governance-submit-draft-route__notice"
          role="alert"
        >
          The list of linkable Polls is unavailable right now. You can still
          draft your proposal, but you will need to link a passed Poll before
          submitting once the list loads.
        </p>
      )}

      <GovernanceNotice
        tone="unavailable"
        title="Draft proposals cannot be submitted from this node"
        detail={`No governance write path is wired for this proposal kind: "draft" is not one of the kinds /api/governance/proposals/:kind accepts, so a submission would 404 before it ever reached catalyrst-governance. The form below is readable, but nothing entered in it can be published from here.`}
      />

      <SubmitDraftWizard
        data={draftData}
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
