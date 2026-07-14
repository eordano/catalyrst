import { useMemo } from "react";

import {
  buildCreateProposal,
  getLinkedWearablesData,
} from "@data/lib/catalyst/governance/submit-linked-wearables";
import { useAuth } from "@data/lib/auth/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { makeCreate } from "@features/stories/governance/submit-linked-wearables/machine";
import GvSubmitLinkedWearablesWizard from "@features/stories/governance/submit-linked-wearables/GvSubmitLinkedWearablesWizard";

import type { Route } from "./+types/governance.submit.linked-wearables";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-linked-wearables";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_lw_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const lw = getLinkedWearablesData();

  const payload = { sid, step, lw, assignment };
  return wrap(payload);
}

export default function GovernanceSubmitLinkedWearablesRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;

  const { identity } = useAuth();
  const create = useMemo(() => makeCreate(buildCreateProposal(identity)), [identity]);

  return (
    <main className="governance-submit-linked-wearables-route">
      <GvSubmitLinkedWearablesWizard
        data={d.lw}
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        initialStep={d.step ?? undefined}
        create={create}
      />
    </main>
  );
}
