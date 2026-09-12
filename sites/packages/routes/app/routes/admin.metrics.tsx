import { useEffect, useRef } from "react";
import { Link } from "react-router";

import AdMetricsPage from "@ui/admin/pages/AdMetricsPage";
import type { AdMetricsSurfaceLink } from "@ui/admin/pages/AdMetricsTypes";
import SitesChrome from "@ui/web/frames/SitesChrome";

import { loadAdminMetrics, type SurfaceKey } from "@data/lib/catalyst/admin/metrics";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/admin.metrics";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "admin/metrics";

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "dashboard",
  flags: { dashboard: true },
  experimentKey: "admin_moderation_metrics",
};

const SURFACES: AdMetricsSurfaceLink[] = [
  { key: "places", label: "Places moderation", deepLink: "/admin/places-moderation" },
  {
    key: "communities",
    label: "Communities moderation",
    deepLink: "/admin/communities-moderation",
  },
  { key: "events", label: "What's On users", deepLink: "/admin/whatson-users" },
];

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    DEFAULT_ASSIGNMENT,
  );

  const metrics = await loadAdminMetrics({ signal: request.signal });

  const payload = { sid, assignment, metrics };

  return wrap(payload);
}

export default function AdminMetricsRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  const ctx = {
    sid: d.sid,
    story: STORY,
    variant: d.assignment.variant,
    experimentKey: d.assignment.experimentKey,
  };

  const liveTiles = d.metrics.tiles.filter((t) => t.kind === "live").length;
  const unavailableTiles = d.metrics.tiles.length - liveTiles;

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track(
      "admin_metrics_viewed",
      { live_tiles: liveTiles, unavailable_tiles: unavailableTiles },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.sid]);

  function onSurfaceClick(surface: SurfaceKey) {
    track("admin_metrics_surface_clicked", { surface }, ctx);
  }

  return (
    <SitesChrome>
      <AdMetricsPage
        tiles={d.metrics.tiles}
        kpis={{
          message: d.metrics.kpis.message,
          fix: d.metrics.kpis.fix,
          serverCheck: d.metrics.kpis.serverCheck,
        }}
        trend={{
          message: d.metrics.trend.message,
          fix: d.metrics.trend.fix,
          serverCheck: d.metrics.trend.serverCheck,
        }}
        funnel={{
          message: d.metrics.funnel.message,
          fix: d.metrics.funnel.fix,
          serverCheck: d.metrics.funnel.serverCheck,
        }}
        generatedAt={d.metrics.generatedAt}
        surfaces={SURFACES}
        onSurfaceClick={onSurfaceClick}
        LinkComponent={Link}
      />
    </SitesChrome>
  );
}
