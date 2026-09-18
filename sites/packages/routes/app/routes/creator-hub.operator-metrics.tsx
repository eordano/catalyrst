import { useEffect, useRef } from "react";
import { useNavigate } from "react-router";

import OperatorMetricsView from "@ui/creatorhub/pages/OperatorMetricsView";

import "@ui/creatorhub/pages/metricsdashboardview.css";
import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useChromeAuth } from "@ui/web/frames/chrome-auth";
import { loadPresenceSnapshot } from "@data/lib/catalyst/places/presence.server";
import { occupancyTotals, sceneJumpUrl, worldHeadcount, worldJumpUrl } from "@data/lib/catalyst/places/presence";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoaderWith } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import { OPERATOR_EVENTS } from "@core/lib/telemetry/operator-events";
import { loadOperatorMetrics } from "@data/lib/catalyst/admin/operator-metrics.server";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.operator-metrics";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Network metrics");

const STORY: StoryId = "admin/operator-metrics";

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "dashboard",
  flags: { dashboard: true, occupancy: true },
  experimentKey: "operator_metrics_dashboard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const {
    sid,
    assignment,
    wrap,
    data: [presence, { funnel, admin }],
  } = await storyLoaderWith(request, STORY, DEFAULT_ASSIGNMENT, () =>
    Promise.all([loadPresenceSnapshot({ signal: request.signal }), loadOperatorMetrics()]),
  );
  const totals = occupancyTotals(presence);

  const payload = { sid, assignment, presence, totals, funnel, admin };
  return wrap(payload);
}

export default function CreatorHubOperatorMetrics({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const { presence, totals, funnel, admin } = d;
  const { isConnected, address } = useAuth();
  const { name } = useChromeAuth();
  const navigate = useNavigate();

  const ctx = {
    sid: d.sid,
    story: STORY,
    variant: d.assignment.variant,
    experimentKey: d.assignment.experimentKey,
  };

  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(OPERATOR_EVENTS.dashboardViewed, { source: presence.source }, ctx);
    track(
      OPERATOR_EVENTS.visitsViewed,
      {
        peers: totals?.peers ?? null,
        scenes: totals?.scenes ?? null,
        worlds: totals?.worlds ?? null,
      },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.sid]);

  function onFunnelClick(target: string) {
    track(OPERATOR_EVENTS.dashboardFunnelClicked, { target }, ctx);
  }

  const sceneRows = (presence.scenes ?? []).map((s) => ({
    key: s.pointer,
    label: s.scene_name || s.pointer,
    meta: s.pointer,
    count: s.count,
    href: sceneJumpUrl(s.pointer),
  }));
  const worldRows = (presence.worlds ?? []).map((w) => ({
    key: w.world_name,
    label: w.world_name,
    meta: w.live_users != null ? `${w.live_users} live` : "",
    count: worldHeadcount(w),
    href: worldJumpUrl(w.world_name),
  }));

  return (
    <OperatorMetricsView
      source={presence.source}
      totals={totals ?? undefined}
      funnel={funnel}
      admin={admin}
      sceneRows={sceneRows}
      worldRows={worldRows}
      guardrailEvent={OPERATOR_EVENTS.placementRejected}
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => {
        openSignIn();
      }}
      onFunnelTab={() => {
        void navigate("/creator-hub/metrics");
      }}
      onFunnelClick={onFunnelClick}
    />
  );
}
