import { useMemo } from "react";

import { useAuth } from "@data/lib/auth/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import {
  buildCreateTender,
  getSubmitTenderData,
  loadPitches,
  resolveLinkedPitch,
} from "@data/lib/catalyst/governance/submit-tender";
import SubmitTenderWizard from "@features/stories/governance/submit-tender/SubmitTenderWizard";

import type { Route } from "./+types/governance.submit.tender";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-tender";

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_tender_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const linkedProposalId = url.searchParams.get("linked_proposal_id")?.trim() || null;

  const vpRaw = url.searchParams.get("vp");
  const vpParsed = vpRaw != null ? Number.parseInt(vpRaw, 10) : NaN;
  const vpOverride = Number.isFinite(vpParsed) && vpParsed >= 0 ? vpParsed : null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    DEFAULT_ASSIGNMENT,
  );

  const tenderData = getSubmitTenderData();
  const pitchList = await loadPitches({ signal: request.signal });
  const pitch = resolveLinkedPitch(pitchList.pitches, linkedProposalId);

  const payload = {
    sid,
    step,
    vpOverride,
    assignment,
    tenderData,
    pitch,
    pitchesSource: pitchList.source,
  };
  return wrap(payload);
}

export default function GovernanceSubmitTender({ loaderData }: Route.ComponentProps) {
  const { sid, step, vpOverride, assignment, tenderData, pitch } = loaderData;

  const auth = useAuth();
  const submit = useMemo(() => buildCreateTender(auth.identity), [auth.identity]);
  const votingPower =
    vpOverride != null
      ? vpOverride
      : auth.isConnected
        ? tenderData.submission_threshold_tender
        : 0;

  return (
    <main className="governance-submit-tender">
      <SubmitTenderWizard
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        data={tenderData}
        pitch={pitch}
        votingPower={votingPower}
        initialStep={step ?? undefined}
        submit={submit}
      />
    </main>
  );
}
