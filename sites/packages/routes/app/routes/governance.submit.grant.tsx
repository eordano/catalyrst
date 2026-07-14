import { loadGrantBudget } from "@data/lib/catalyst/governance/grant-budget";
import { governanceAsOfLabel } from "@data/lib/catalyst/governance/freshness";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GovernanceNotice from "@features/components/governance/GovernanceNotice";
import SubmitGrantWizard from "@features/stories/governance/submit-grant/SubmitGrantWizard";

import type { Route } from "./+types/governance.submit.grant";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-grant";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_grant_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    "governance/submit-grant",
    FALLBACK,
  );

  const budget = await loadGrantBudget({ signal: request.signal });

  const payload = { sid, budget, assignment };

  return wrap(payload);
}

export default function GovernanceSubmitGrantRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { sid, budget, assignment } = d;

  return (
    <main className="governance-submit-grant-route">
      <GovernanceNotice
        tone="unavailable"
        title="Grant proposals cannot be submitted from this node"
        detail={`No governance write path is wired for this proposal kind: "grant" is not one of the kinds /api/governance/proposals/:kind accepts, so a submission would 404 before it ever reached catalyrst-governance. The form below is readable, but nothing entered in it can be published from here.`}
      />

      {budget.source === "unavailable" ? (
        <GovernanceNotice
          tone="unavailable"
          title="Grant budget figures are unavailable"
          detail={budget.reason}
        />
      ) : budget.asOf ? (
        <GovernanceNotice
          tone="stale"
          title={`Budget figures as of ${governanceAsOfLabel(budget.asOf)}`}
          detail="This node serves a mirror of the DAO budget. Its background sync is not enabled, so these figures are only as fresh as the last manual sync."
        />
      ) : null}

      <SubmitGrantWizard
        budget={budget}
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
