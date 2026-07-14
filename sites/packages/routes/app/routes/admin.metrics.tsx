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

/**
 * The fixture no longer ships as numbers.
 *
 * `loadAdminMetrics` returns two live counts (approved / featured events, from
 * the public `GET /events/api/events?list=all` --
 * `catalyrst-events/src/handlers/events.rs:345-362`, `optional_user`, no gate)
 * and an explicit unavailable state for everything else. This route renders
 * exactly that; it does not fill gaps.
 *
 * `src/fixtures/admin-metrics.json` is not deleted. It is reachable only
 * through `loadSampleAdminMetrics()`, which is off by default, is not called
 * from here, and requires the caller to render a persistent "sample data"
 * banner.
 *
 * `operator-metrics.server.ts` is deliberately not used as a substitute: it is
 * creator-hub-owned, and it serves aggregate telemetry to any visitor with no
 * authorization at all. That is a finding to report, not a data source.
 *
 * The 7d/30d range toggle is gone. Nothing on the page is windowed, so it was a
 * control over data that does not exist.
 */
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
