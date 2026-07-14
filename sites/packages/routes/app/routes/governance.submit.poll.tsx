import { useAuth } from "@data/lib/auth/index";
import { shortAddress } from "@data/lib/catalyst/format/address";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GovernanceNotice from "@features/components/governance/GovernanceNotice";
import SubmitPollWizard from "@features/stories/governance/submit-poll/SubmitPollWizard";
import type { GateInput } from "@features/stories/governance/submit-poll/machine";

import type { Route } from "./+types/governance.submit.poll";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-poll";

const VP_REQUIRED = 100;

function connectedVotingPower(): number | null {
  return null;
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_submit_poll_flow",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const connectedOverride = url.searchParams.get("connected") === "0";
  const vpOverride = url.searchParams.get("vp") === "0";

  const payload = {
    sid,
    step,
    assignment,
    connectedOverride,
    vpOverride,
  };
  return wrap(payload);
}

export default function GovernanceSubmitPoll({ loaderData }: Route.ComponentProps) {
  const { sid, step, assignment, connectedOverride, vpOverride } =
    loaderData;

  const auth = useAuth();
  const connected = !connectedOverride && auth.isConnected;
  const vp = connectedVotingPower();
  const hasVp =
    !vpOverride && connected && (vp == null || vp >= VP_REQUIRED);
  const gate: GateInput = { connected, hasVp };

  const account =
    connected && auth.address ? shortAddress(auth.address) : undefined;

  return (
    <main className="governance-submit-poll-route">
      <GovernanceNotice
        tone="unavailable"
        title="Poll proposals cannot be submitted from this node"
        detail={`No governance write path is wired for this proposal kind: "poll" is not one of the kinds /api/governance/proposals/:kind accepts, so a submission would 404 before it ever reached catalyrst-governance. The form below is readable, but nothing entered in it can be published from here.`}
      />

      <SubmitPollWizard
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        gate={gate}
        account={account}
        initialStep={step ?? undefined}
      />
    </main>
  );
}
