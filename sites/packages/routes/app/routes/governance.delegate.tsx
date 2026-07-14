import { loadDelegateData } from "@data/lib/catalyst/governance/delegate-vp";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import DelegateVpWizard from "@features/stories/governance/delegate-vp/DelegateVpWizard";

import type { Route } from "./+types/governance.delegate";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/delegate-vp";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_delegate_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    "governance/delegate-vp",
    FALLBACK,
  );

  const url = new URL(request.url);
  const address = url.searchParams.get("wallet")?.trim() || readWallet(request);

  const delegateData = await loadDelegateData({
    address,
    signal: request.signal,
  });

  const payload = { sid, delegateData, assignment };

  return wrap(payload);
}

export default function GovernanceDelegateRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { sid, delegateData, assignment } = d;

  return (
    <main className="governance-delegate-route">
      <DelegateVpWizard
        data={delegateData}
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
