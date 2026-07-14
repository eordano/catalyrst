import { Link } from "react-router";
import { href } from "@core/lib/router/routes";

import GvNotFound from "@ui/governance/pages/GvNotFound";

import { useAuth } from "@data/lib/auth/index";
import { toPoiRequest, getPoiSubmitContext, type PoiAccount, type PoiSubmitContext } from "@data/lib/catalyst/governance/submit-poi";
import { shortAddress } from "@data/lib/catalyst/format/address";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import GovernanceNotice from "@features/components/governance/GovernanceNotice";
import GvSubmitPOIWizard from "@features/stories/governance/submit-poi/GvSubmitPOIWizard";

import type { Route } from "./+types/governance.submit.poi";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "governance/submit-poi";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "gv_poi_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const poiRequest = toPoiRequest(url.searchParams.get("request")) ?? "add";

  const connectedOverride = url.searchParams.get("connected") === "0";
  const vpOverride = url.searchParams.get("vp") === "0";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const payload = {
    sid,
    step,
    notFound: false as const,
    ctx: getPoiSubmitContext(poiRequest),
    connectedOverride,
    vpOverride,
    assignment,
  };
  return wrap(payload);
}

export default function GovernanceSubmitPoi({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  const auth = useAuth();
  const connected = !d.connectedOverride && auth.isConnected;
  const address = connected && auth.address ? auth.address.toLowerCase() : "";
  const label = address ? shortAddress(address) : "Guest";
  const eligible = connected && !d.vpOverride;
  const votingPower: number | null = !connected ? 0 : d.vpOverride ? 0 : null;
  const account: PoiAccount = { address, label, votingPower };

  if (d.notFound || !d.ctx || !d.assignment) {
    return (
      <main className="governance-submit-poi-route governance-submit-poi-route--notfound">
        <GvNotFound
          title="Page not found"
          description="The proposal type you are looking for doesn't exist. Choose to add or remove a Point of Interest from the proposal menu."
        />
      </main>
    );
  }

  const ctx: PoiSubmitContext = { ...d.ctx, account };

  return (
    <main className="governance-submit-poi-route">
      <nav className="governance-submit-poi-route__crumbs" aria-label="Breadcrumb">
        <Link prefetch="intent" to={href("/governance")}>
          Governance
        </Link>
        <span aria-hidden="true"> / </span>
        <Link prefetch="intent" to={href("/governance/proposals")}>
          Proposals
        </Link>
        <span aria-hidden="true"> / </span>
        <span aria-current="page">{ctx.title}</span>
      </nav>

      {!eligible && (
        <p className="governance-submit-poi-route__notice" role="status">
          {connected
            ? "You don\u{2019}t have enough Voting Power to submit this proposal. Buy MANA to get VP, or run for a delegate."
            : "Sign in to submit a Point of Interest proposal. You can review the form below in the meantime."}
        </p>
      )}

      <GovernanceNotice
        tone="unavailable"
        title="Point of Interest proposals cannot be submitted from this node"
        detail={`No governance write path is wired for this proposal kind: "poi" is not one of the kinds /api/governance/proposals/:kind accepts, so a submission would 404 before it ever reached catalyrst-governance. The form below is readable, but nothing entered in it can be published from here.`}
      />

      <GvSubmitPOIWizard
        ctx={ctx}
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
