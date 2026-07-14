import type { ComponentType } from "react";

import type {
  MetricsLinkProps,
  Range,
  SurfaceKey,
  SurfaceKpi,
} from "./AdMetricsTypes";

type Props = {
  kpis: SurfaceKpi[];
  range: Range;
  onSurfaceClick: (surface: SurfaceKey) => void;
  LinkComponent?: ComponentType<MetricsLinkProps>;
};

function slaLabel(hours: number): string {
  if (hours <= 0) return "\u{2014}";
  if (hours < 48) return `${hours.toFixed(1)}h`;
  return `${(hours / 24).toFixed(1)}d`;
}

function pct(n: number): string {
  return `${Math.round(n * 100)}%`;
}

export default function AdMetricsDashboard({
  kpis,
  range,
  onSurfaceClick,
  LinkComponent = undefined,
}: Props) {
  return (
    <div className="am__cards">
      {kpis.map((k) => {
        const decided = k.approvedish + k.dismissedish;
        const posPct = decided > 0 ? (k.approvedish / decided) * 100 : 0;
        const body = (
          <>
            <div className="am-card__top">
              <span className="am-card__label">{k.label}</span>
              <span
                className={
                  "am-badge " + (k.live ? "am-badge--live" : "am-badge--fixture")
                }
              >
                {k.live ? "live" : "snapshot"}
              </span>
            </div>

            <div className="am-card__depth">
              <b>{k.openDepth}</b>
              <span>open in queue</span>
            </div>

            <div className="am-card__stats">
              <div>
                <span className="am-stat__k">Decisions ({range})</span>
                <span className="am-stat__v">{k.decisions}</span>
              </div>
              <div>
                <span className="am-stat__k">Approval</span>
                <span className="am-stat__v">{pct(k.approvalRate)}</span>
              </div>
              <div>
                <span className="am-stat__k">Median SLA</span>
                <span className="am-stat__v">{slaLabel(k.medianSlaHours)}</span>
              </div>
            </div>

            <div
              className="am-split"
              role="img"
              aria-label={`${k.approvedish} approved, ${k.dismissedish} dismissed`}
            >
              <span className="am-split__pos" style={{ width: `${posPct}%` }} />
              <span
                className="am-split__neg"
                style={{ width: `${100 - posPct}%` }}
              />
            </div>

            <div className="am-card__cta">Open moderation &#x2192;</div>
          </>
        );
        return LinkComponent ? (
          <LinkComponent
            key={k.key}
            to={k.deepLink}
            prefetch="intent"
            className="am-card"
            onClick={() => onSurfaceClick(k.key)}
            aria-label={`Open ${k.label} moderation`}
          >
            {body}
          </LinkComponent>
        ) : (
          <a
            key={k.key}
            href={k.deepLink}
            className="am-card"
            onClick={() => onSurfaceClick(k.key)}
            aria-label={`Open ${k.label} moderation`}
          >
            {body}
          </a>
        );
      })}
    </div>
  );
}
