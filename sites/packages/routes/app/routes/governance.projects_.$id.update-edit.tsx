import GvNotFound from "@ui/governance/pages/GvNotFound";

import {
  loadEditUpdate,
  type EditUpdateData,
} from "@data/lib/catalyst/governance/edit-project-update";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GovernanceNotice from "@features/components/governance/GovernanceNotice";
import GvEditProjectUpdate from "@features/stories/governance/edit-project-update/GvEditProjectUpdate";

import type { Route } from "./+types/governance.projects_.$id.update-edit";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/edit-project-update";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_update_edit_wizard",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const { id } = params;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    "governance/edit-project-update",
    FALLBACK,
  );

  const edit = await loadEditUpdate(id ?? "", { signal: request.signal });

  // loadEditUpdate already blanks the record and carries the reason when it
  // cannot serve one. Nothing to launder here.
  const safeEdit: EditUpdateData = edit;

  const payload = { sid, edit: safeEdit, assignment };

  return wrap(payload);
}

export default function GovernanceEditProjectUpdateRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { sid, edit, assignment } = d;

  if (edit.source !== "live") {
    return (
      <main className="governance-edit-project-update-route governance-edit-project-update-route--notfound">
        <GovernanceNotice
          tone="unavailable"
          title="There is no project update to edit here"
          detail={
            edit.reason ??
            "This node could not serve the update behind this page."
          }
        />
        <GvNotFound />
      </main>
    );
  }

  return (
    <main className="governance-edit-project-update-route">
      <GvEditProjectUpdate
        data={edit}
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
