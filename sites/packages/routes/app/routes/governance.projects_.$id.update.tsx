import {
  loadProjectUpdateContext,
  type ProjectUpdateContext,
} from "@data/lib/catalyst/governance/project-update";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GovernanceNotice from "@features/components/governance/GovernanceNotice";
import SubmitProjectUpdateWizard from "@features/stories/governance/submit-project-update/SubmitProjectUpdateWizard";
import GvNotFound from "@ui/governance/pages/GvNotFound";

import type { Route } from "./+types/governance.projects_.$id.update";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-project-update";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_project_update_wizard",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const { id } = params;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    "governance/submit-project-update",
    FALLBACK,
  );

  const rawContext = await loadProjectUpdateContext(id, { signal: request.signal });

  // loadProjectUpdateContext already blanks the project/funding/updates when it
  // cannot reach the node, and carries the reason. Nothing to launder here.
  const context: ProjectUpdateContext = rawContext;

  const payload = { id, sid, context, assignment };

  return wrap(payload);
}

export default function GovernanceSubmitProjectUpdateRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { sid, context, assignment } = d;

  if (context.source !== "live") {
    return (
      <main className="governance-submit-project-update-route governance-submit-project-update-route--notfound">
        <GovernanceNotice
          tone="unavailable"
          title="This project update form is unavailable"
          detail={
            context.reason ??
            "This node could not serve the project behind this update."
          }
        />
        <GvNotFound />
      </main>
    );
  }

  return (
    <main className="governance-submit-project-update-route">
      <SubmitProjectUpdateWizard
        context={context}
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
