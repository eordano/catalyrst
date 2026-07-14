import path from "node:path";
import { useMemo } from "react";

import {
  buildCreateProposal,
  getGovernanceProposalCopy,
  loadLinkedDrafts,
  type GovernanceProposalCopy,
  type LinkedDraft,
} from "@data/lib/catalyst/governance/submit-governance-proposal";
import { useAuth } from "@data/lib/auth/index";
import { resolveAssignment, type Assignment } from "@core/lib/experiments/assign";
import { sidLoader } from "@core/lib/experiments/story-loader";
import { parseStory } from "@core/lib/experiments/context";
import { trackExposure } from "@core/lib/telemetry/track";
import GovProposalWizard from "@features/stories/governance/submit-governance-proposal/GovProposalWizard";

import type { Route } from "./+types/governance.submit.governance";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-governance-proposal";
const STORY_DIR = path.join(process.cwd(), "packages", "features", "src", "stories", STORY);
const EXPERIMENT_KEY = "gv_govprop_wizard";

const DEFAULT_VP = 3120;

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const url = new URL(request.url);

  let drafts: LinkedDraft[] = [];
  let live = false;
  let draftsReason: string | null = null;
  try {
    const res = await loadLinkedDrafts({ signal: request.signal });
    drafts = res.drafts;
    live = res.live;
    draftsReason = res.reason;
  } catch (err) {
    draftsReason = err instanceof Error ? err.message : String(err);
  }

  const copy: GovernanceProposalCopy = getGovernanceProposalCopy();

  const vpRaw = url.searchParams.get("vp");
  const vpParsed = vpRaw != null ? Number(vpRaw) : NaN;
  const votingPower = Number.isFinite(vpParsed) && vpParsed >= 0 ? vpParsed : DEFAULT_VP;

  let assignment: Assignment = {
    variant: "wizard",
    flags: { wizard: true },
    experimentKey: EXPERIMENT_KEY,
  };
  try {
    const story = parseStory(STORY_DIR);
    assignment = await resolveAssignment(request, story);
  } catch {
  }

  trackExposure({
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  });

  const payload = { sid, drafts, live, draftsReason, copy, votingPower, assignment };

  return wrap(payload);
}

export default function GovernanceSubmitGovernanceRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;

  const { identity } = useAuth();
  const submit = useMemo(() => buildCreateProposal(identity), [identity]);

  return (
    <main className="governance-submit-governance-route">
      <GovProposalWizard
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        copy={d.copy}
        drafts={d.drafts}
        live={d.live}
        votingPower={d.votingPower}
        submit={submit}
      />
    </main>
  );
}
