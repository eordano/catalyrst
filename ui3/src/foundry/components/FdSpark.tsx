import "./fdspark.css";

type FdSparkProps = {
  values: readonly number[];
  w?: number;
  h?: number;
  ariaLabel?: string;
  className?: string;
};

/** One neutral series. The last reading is the only emphasised point. */
export default function FdSpark({
  values,
  w = 108,
  h = 28,
  ariaLabel,
  className = "",
}: FdSparkProps) {
  if (values.length === 0) return null;

  const pad = 3;
  const max = Math.max(...values);
  const min = Math.min(...values);
  const span = max - min || 1;
  const step = values.length > 1 ? (w - pad * 2) / (values.length - 1) : 0;

  const points = values.map((v, i) => {
    const x = pad + i * step;
    const y = h - pad - ((v - min) / span) * (h - pad * 2);
    return { x, y };
  });

  const last = points[points.length - 1];
  const label =
    ariaLabel ?? `Trend, ${values.length} readings, latest ${values[values.length - 1]}`;

  return (
    <svg
      className={"fd-spark" + (className ? " " + className : "")}
      width={w}
      height={h}
      viewBox={`0 0 ${w} ${h}`}
      role="img"
      aria-label={label}
    >
      <polyline
        className="fd-spark__line"
        fill="none"
        points={points.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ")}
      />
      {last ? <circle className="fd-spark__last" cx={last.x} cy={last.y} r="2.6" /> : null}
    </svg>
  );
}
