import type { TrendSeries } from "./AdMetricsTypes";

type Props = {
  labels: string[];
  series: TrendSeries[];
};

const W = 240;
const H = 38;
const PAD = 3;

function pointsFor(values: number[]): string {
  if (values.length === 0) return "";
  const max = Math.max(...values, 1);
  const min = Math.min(...values, 0);
  const span = max - min || 1;
  const step = values.length > 1 ? (W - PAD * 2) / (values.length - 1) : 0;
  return values
    .map((v, i) => {
      const x = PAD + i * step;
      const y = H - PAD - ((v - min) / span) * (H - PAD * 2);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

export default function AdQueueDepthTrend({ labels, series }: Props) {
  const rangeLabel =
    labels.length >= 2
      ? `${labels[0]} \u{2013} ${labels[labels.length - 1]}`
      : "recent";

  return (
    <div className="am-panel">
      <h2 className="am-panel__title">Open-queue depth &#xB7; {rangeLabel}</h2>
      <div className="am-spark">
        {series.map((s) => {
          const now = s.points[s.points.length - 1] ?? 0;
          const first = s.points[0] ?? 0;
          const delta = now - first;
          return (
            <div className="am-spark__row" key={s.key}>
              <span className="am-spark__label">{s.label}</span>
              <svg
                viewBox={`0 0 ${W} ${H}`}
                preserveAspectRatio="none"
                role="img"
                aria-label={`${s.label} queue depth trend, now ${now}, change ${delta >= 0 ? "+" : ""}${delta}`}
              >
                <polyline
                  points={pointsFor(s.points)}
                  fill="none"
                  stroke={delta <= 0 ? "#34c759" : "#ff9f0a"}
                  strokeWidth={2}
                  strokeLinejoin="round"
                  strokeLinecap="round"
                />
              </svg>
              <span className="am-spark__now">{now}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
