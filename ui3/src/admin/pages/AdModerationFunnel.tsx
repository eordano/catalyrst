import type { FunnelRow } from "./AdMetricsTypes";

type Props = {
  rows: FunnelRow[];
};

function widthPct(value: number, base: number): number {
  if (base <= 0) return 0;
  return Math.max(2, Math.min(100, (value / base) * 100));
}

export default function AdModerationFunnel({ rows }: Props) {
  return (
    <div className="am-panel">
      <h2 className="am-panel__title">
        Funnel &#xB7; reported &#x2192; reviewed &#x2192; resolved
      </h2>
      <div className="am-funnel">
        {rows.map((r) => {
          const reviewedRate =
            r.reported > 0 ? Math.round((r.reviewed / r.reported) * 100) : 0;
          const resolvedRate =
            r.reported > 0
              ? Math.round((r.resolvedOrActioned / r.reported) * 100)
              : 0;
          return (
            <div className="am-funnel__row" key={r.key}>
              <span className="am-funnel__label">
                {r.label} &#xB7; {resolvedRate}% resolved
              </span>
              <div className="am-funnel__bars">
                <div
                  className="am-funnel__bar"
                  style={{ width: `${widthPct(r.reported, r.reported)}%` }}
                >
                  <small>Reported</small>
                  <span>{r.reported}</span>
                </div>
                <div
                  className="am-funnel__bar am-funnel__bar--reviewed"
                  style={{ width: `${widthPct(r.reviewed, r.reported)}%` }}
                >
                  <small>Reviewed &#xB7; {reviewedRate}%</small>
                  <span>{r.reviewed}</span>
                </div>
                <div
                  className="am-funnel__bar am-funnel__bar--resolved"
                  style={{
                    width: `${widthPct(r.resolvedOrActioned, r.reported)}%`,
                  }}
                >
                  <small>Resolved / actioned</small>
                  <span>{r.resolvedOrActioned}</span>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
