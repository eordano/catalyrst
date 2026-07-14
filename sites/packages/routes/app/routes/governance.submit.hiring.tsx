import { useMemo } from "react";

import GvNotFound from "@ui/governance/pages/GvNotFound";

import {
  buildCreateProposal,
  toHiringRequest,
  loadHiringSubmitContext,
} from "@data/lib/catalyst/governance/submit-hiring";
import { useAuth } from "@data/lib/auth/index";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { makeSubmit } from "@features/stories/governance/submit-hiring/machine";
import GvSubmitHiringWizard from "@features/stories/governance/submit-hiring/GvSubmitHiringWizard";

import type { Route } from "./+types/governance.submit.hiring";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-hiring";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_hiring_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const hiringRequest = toHiringRequest(url.searchParams.get("request")) ?? "add";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const payload = {
    sid,
    step,
    notFound: false as const,
    ctx: await loadHiringSubmitContext(hiringRequest, { signal: request.signal }),
    assignment,
  };
  return wrap(payload);
}

export default function GovernanceSubmitHiring({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  const { identity } = useAuth();
  const submit = useMemo(() => makeSubmit(buildCreateProposal(identity)), [identity]);

  if (d.notFound || !d.ctx || !d.assignment) {
    return (
      <main className="governance-submit-hiring-route governance-submit-hiring-route--notfound">
        <GvNotFound
          title="Page not found"
          description="The proposal type you are looking for doesn't exist. Choose to add or remove a Committee member from the proposal menu."
        />
      </main>
    );
  }

  return (
    <main className="governance-submit-hiring-route">
      <GvSubmitHiringWizard
        ctx={d.ctx}
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        initialStep={d.step ?? undefined}
        submit={submit}
      />
    </main>
  );
}
