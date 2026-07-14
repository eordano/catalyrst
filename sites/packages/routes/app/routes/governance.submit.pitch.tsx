import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import { useAuth } from "@data/lib/auth/index";
import { getPitchSubmitContext } from "@data/lib/catalyst/governance/submit-pitch";
import { shortAddress } from "@data/lib/catalyst/format/address";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GvSubmitPitchWizard, {
  type GateInput,
} from "@features/stories/governance/submit-pitch/GvSubmitPitchWizard";

import GovernanceNotice from "@features/components/governance/GovernanceNotice";
import type { Route } from "./+types/governance.submit.pitch";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-pitch";

const VP_REQUIRED = 100;

function connectedVotingPower(): number | null {
  return null;
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_pitch_wizard",
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
    ctx: getPitchSubmitContext(),
    assignment,
    connectedOverride,
    vpOverride,
  };
  return wrap(payload);
}

export default function GovernanceSubmitPitch({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  const auth = useAuth();
  const connected = !d.connectedOverride && auth.isConnected;
  const vp = connectedVotingPower();
  const hasVp = !d.vpOverride && connected && (vp == null || vp >= VP_REQUIRED);
  const gate: GateInput = { connected, hasVp };
  const account =
    connected && auth.address ? shortAddress(auth.address) : undefined;

  return (
    <main className="governance-submit-pitch-route">
      <nav className="governance-submit-pitch-route__crumbs" aria-label="Breadcrumb">
        <Link prefetch="intent" to={href("/governance")}>
          Governance
        </Link>
        <span aria-hidden="true"> / </span>
        <Link prefetch="intent" to={href("/governance/proposals")}>
          Proposals
        </Link>
        <span aria-hidden="true"> / </span>
        <span aria-current="page">Submit Pitch</span>
      </nav>

      <GovernanceNotice
        tone="unavailable"
        title="Pitch proposals cannot be submitted from this node"
        detail={`No governance write path is wired for this proposal kind: "pitch" is not one of the kinds /api/governance/proposals/:kind accepts, so a submission would 404 before it ever reached catalyrst-governance. The form below is readable, but nothing entered in it can be published from here.`}
      />

      <GvSubmitPitchWizard
        ctx={d.ctx}
        gate={gate}
        account={account}
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
        initialStep={d.step ?? undefined}
      />
    </main>
  );
}
