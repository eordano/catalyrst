import { useMemo } from "react";

import GvNotFound from "@ui/governance/pages/GvNotFound";

import {
  buildCreateProposal,
  getSubmitCatalystData,
  toCatalystRequest,
  type CatalystRequest,
  type SubmitCatalystData,
} from "@data/lib/catalyst/governance/submit-catalyst";
import { useAuth } from "@data/lib/auth/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { makeCreate } from "@features/stories/governance/submit-catalyst/machine";
import SubmitCatalystWizard from "@features/stories/governance/submit-catalyst/SubmitCatalystWizard";

import type { Route } from "./+types/governance.submit.catalyst";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-catalyst";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_catalyst_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;

  const requestKind = toCatalystRequest(url.searchParams.get("request")) ?? "add";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const submitData = getSubmitCatalystData();

  const payload = {
    notFound: false as const,
    sid,
    step,
    request: requestKind,
    assignment,
    data: submitData,
  };
  return wrap(payload);
}

type LoaderData =
  | { notFound: true; sid: string }
  | {
      notFound: false;
      sid: string;
      step: string | null;
      request: CatalystRequest;
      assignment: Assignment;
      data: SubmitCatalystData;
    };

export default function GovernanceSubmitCatalyst({ loaderData }: Route.ComponentProps) {
  const d = loaderData as LoaderData;

  const { identity } = useAuth();
  const create = useMemo(() => makeCreate(buildCreateProposal(identity)), [identity]);

  if (d.notFound) {
    return <GvNotFound />;
  }

  return (
    <main className="governance-submit-catalyst">
      <SubmitCatalystWizard
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        request={d.request}
        data={d.data}
        initialStep={d.step ?? undefined}
        create={create}
      />
    </main>
  );
}
