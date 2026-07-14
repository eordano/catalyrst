import type { ComponentType } from "react";

import AdControlNotice from "./AdControlNotice";
import type {
  AdMetricsBlock,
  AdMetricsSurfaceLink,
  AdMetricTile,
  MetricsLinkProps,
  SurfaceKey,
} from "./AdMetricsTypes";
import "./adminmetrics.css";

/**
 * Moderation metrics.
 *
 * This page used to render `src/fixtures/admin-metrics.json` -- queue depths,
 * approval rates, medians, a trend chart and a funnel -- inside admin chrome,
 * where it read as production telemetry. The fixture's own `_source` field says
 * the counts are synthetic.
 *
 * So the numbers do not ship. What ships is:
 *   - the two genuinely live counts, from the public events list
 *     (`catalyrst-events/src/handlers/events.rs:345-362`, `optional_user`),
 *     each labelled with where it came from;
 *   - an explicit empty state for every other tile and for all three panels,
 *     carrying the reason. Not a zero, not a dash with a sparkline -- both still
 *     read as a measurement.
 *
 * There is no time-range toggle any more: nothing here is windowed, so a 7d/30d
 * switch was a control over data that does not exist.
 *
 * `AdMetricsDashboard`, `AdQueueDepthTrend` and `AdModerationFunnel` are left
 * in this package unchanged. They are the layout for the day a real
 * aggregation endpoint exists; they are simply not fed a fixture in the
 * meantime.
 */
export type AdMetricsPageProps = {
  tiles: AdMetricTile[];
  kpis: AdMetricsBlock;
  trend: AdMetricsBlock;
  funnel: AdMetricsBlock;
  generatedAt: string;
  surfaces: AdMetricsSurfaceLink[];
  onSurfaceClick: (surface: SurfaceKey) => void;
  LinkComponent?: ComponentType<MetricsLinkProps>;
};

function PlainLink({ to, children, ...rest }: MetricsLinkProps) {
  return (
    <a href={to} {...rest}>
      {children}
    </a>
  );
}

export default function AdMetricsPage({
  tiles,
  kpis,
  trend,
  funnel,
  generatedAt,
  surfaces,
  onSurfaceClick,
  LinkComponent = undefined,
}: AdMetricsPageProps) {
  const Anchor = LinkComponent ?? PlainLink;

  return (
    <div className="am">
      <div className="am__head">
        <div>
          <h1 className="am__title">Moderation metrics</h1>
          <p className="am__sub">
            Every figure on this page states where it came from. Anything
            without a source is shown as unavailable rather than as a number &#x2014;
            this node has no moderation-aggregation endpoint, and the counts
            that used to fill this dashboard came from a bundled fixture.
          </p>
        </div>
      </div>

      <div className="am__tiles">
        {tiles.map((t) =>
          t.kind === "live" ? (
            <div className="am-tile" key={t.key}>
              <span className="am-tile__label">{t.label}</span>
              <span className="am-tile__value">
                {t.value.toLocaleString("en-US")}
              </span>
              <span className="am-tile__source">{t.source}</span>
            </div>
          ) : (
            <div className="am-tile am-tile--unavailable" key={t.key}>
              <span className="am-tile__label">{t.label}</span>
              <span className="am-tile__reason">{t.reason}</span>
            </div>
          ),
        )}
      </div>

      <nav className="am__links" aria-label="Moderation consoles">
        {surfaces.map((s) => (
          <Anchor
            key={s.key}
            to={s.deepLink}
            prefetch="intent"
            onClick={() => onSurfaceClick(s.key)}
          >
            {s.label}
          </Anchor>
        ))}
      </nav>

      <AdControlNotice
        title="Moderation KPIs"
        message={kpis.message}
        fix={kpis.fix}
        serverCheck={kpis.serverCheck}
      />
      <AdControlNotice
        title="Decision trend"
        message={trend.message}
        fix={trend.fix}
        serverCheck={trend.serverCheck}
      />
      <AdControlNotice
        title="Moderation funnel"
        message={funnel.message}
        fix={funnel.fix}
        serverCheck={funnel.serverCheck}
      />

      <p className="am__footnote">
        Page rendered {new Date(generatedAt).toUTCString()}. That is when this
        request was served, not when any figure was measured.
      </p>
    </div>
  );
}
