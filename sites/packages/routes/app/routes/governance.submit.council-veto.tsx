import { useMemo } from "react";

import {
  buildCreateProposal,
  getSubmitCouncilVetoData,
} from "@data/lib/catalyst/governance/submit-council-veto";
import { useAuth } from "@data/lib/auth/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { makeCreate } from "@features/stories/governance/submit-council-veto/machine";
import GvSubmitCouncilVetoWizard from "@features/stories/governance/submit-council-veto/GvSubmitCouncilVetoWizard";

import type { Route } from "./+types/governance.submit.council-veto";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-council-veto";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_council_veto_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const payload = {
    sid,
    step,
    veto: getSubmitCouncilVetoData(),
    assignment,
  };
  return wrap(payload);
}

export default function GovernanceSubmitCouncilVeto({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  const { identity } = useAuth();
  const create = useMemo(() => makeCreate(buildCreateProposal(identity)), [identity]);

  return (
    <main className="governance-submit-council-veto-route">
      <GvSubmitCouncilVetoWizard
        data={d.veto}
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
