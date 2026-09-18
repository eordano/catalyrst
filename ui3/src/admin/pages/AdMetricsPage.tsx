import type { ComponentType, ReactNode } from "react";

import AdControlNotice from "./AdControlNotice";
import type {
  AdMetricsBlock,
  AdMetricsSurfaceLink,
  AdMetricTile,
  MetricsLinkProps,
  SurfaceKey,
} from "./AdMetricsTypes";
import "../../atoms/button.css";
import "../admin.css";
import "./adminmetrics.css";

type AdMetricsPageProps = {
  tiles: AdMetricTile[];
  kpis: AdMetricsBlock;
  trend: AdMetricsBlock;
  funnel: AdMetricsBlock;
  generatedAt: string;
  surfaces: AdMetricsSurfaceLink[];
  onSurfaceClick: (surface: SurfaceKey) => void;
  LinkComponent?: ComponentType<MetricsLinkProps>;
  nav?: ReactNode;
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
  nav = undefined,
}: AdMetricsPageProps) {
  const Anchor = LinkComponent ?? PlainLink;

  return (
    <div className="adm">
      {nav ? (
        <nav className="adm__nav" aria-label="Admin consoles">
          {nav}
        </nav>
      ) : null}
      <div className="adm__page">
        <div className="adm__inner">
          <div className="adm__head">
            <div>
              <h1 className="adm__title">Moderation metrics</h1>
              <p className="adm__sub">
                Every figure on this page states where it came from. Anything
                without a source is shown as unavailable rather than as a number &#x2014;
                this node has no moderation-aggregation endpoint, and the counts
                that used to fill this dashboard came from a bundled fixture.
              </p>
            </div>
          </div>

          <div className="adm-grid">
            {tiles.map((t) =>
              t.kind === "live" ? (
                <div className="adm-card" key={t.key}>
                  <span className="adm-kpi__l">{t.label}</span>
                  <span className="adm-kpi__n">{t.value.toLocaleString("en-US")}</span>
                  <small className="adm-dim">{t.source}</small>
                </div>
              ) : (
                <div className="adm-card adm-card--dashed" key={t.key}>
                  <span className="adm-kpi__l">{t.label}</span>
                  <p className="adm-card__text adm-dim">{t.reason}</p>
                </div>
              ),
            )}
          </div>

          <nav className="adm-actions adm-actions--start" aria-label="Moderation consoles">
            {surfaces.map((s) => (
              <Anchor
                key={s.key}
                to={s.deepLink}
                prefetch="intent"
                className="btn btn--ghost btn--sm"
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

          <p className="adm-metrics__foot">
            Page rendered {new Date(generatedAt).toUTCString()}. That is when this
            request was served, not when any figure was measured.
          </p>
        </div>
      </div>
    </div>
  );
}
