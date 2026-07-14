import { getBanNameSubmitContext } from "@data/lib/catalyst/governance/submit-ban-name";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GovernanceNotice from "@features/components/governance/GovernanceNotice";
import SubmitBanNameWizard from "@features/components/governance/SubmitBanNameWizard";

import type { Route } from "./+types/governance.submit.ban-name";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-ban-name";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_ban_name_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    "governance/submit-ban-name",
    FALLBACK,
  );

  const ctx = getBanNameSubmitContext();

  const payload = { sid, ctx, assignment };

  return wrap(payload);
}

export default function GovernanceSubmitBanNameRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { sid, ctx, assignment } = d;

  return (
    <main className="governance-submit-ban-name-route">
      <GovernanceNotice
        tone="unavailable"
        title="Name ban proposals cannot be submitted from this node"
        detail={`No governance write path is wired for this proposal kind: "ban-name" is not one of the kinds /api/governance/proposals/:kind accepts, so a submission would 404 before it ever reached catalyrst-governance. The form below is readable, but nothing entered in it can be published from here.`}
      />

      <SubmitBanNameWizard
        ctx={ctx}
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
