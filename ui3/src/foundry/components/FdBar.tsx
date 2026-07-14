import type { CSSProperties } from "react";
import "./fdbar.css";

export type FdBarSegment = {
  label: string;
  value: number;
  /** `grant` detaches as an outline, `sim` is purple-outlined and never blended. */
  kind?: "grant" | "sim";
};

type FdBarProps = {
  value?: number;
  max: number;
  label?: string;
  segments?: readonly FdBarSegment[];
  format?: (v: number) => string;
  className?: string;
};

const FILL_STEPS = ["var(--fill-2)", "var(--fill-3)", "var(--fill-4)", "var(--fill-5)"];

const pct = (v: number, max: number) =>
  max > 0 ? Math.max(0, Math.min(100, (v / max) * 100)) : 0;

const tone = (i: number, count: number) =>
  count === 1 ? "var(--fill-5)" : FILL_STEPS[i % FILL_STEPS.length];

export default function FdBar({
  value,
  max,
  label,
  segments,
  format = (v) => String(v),
  className = "",
}: FdBarProps) {
  const parts = segments ?? (value != null ? [{ label: label ?? "", value }] : []);
  const total = parts.reduce((sum, s) => sum + s.value, 0);

  return (
    <div className={"fd-bar" + (className ? " " + className : "")}>
      {label ? (
        <div className="fd-bar__head">
          <span className="fd-bar__label">{label}</span>
          <span className="fd-bar__total">{format(value ?? total)}</span>
        </div>
      ) : null}

      <div className="fd-bar__track">
        {parts.map((seg, i) => {
          const style: CSSProperties = { width: pct(seg.value, max) + "%" };
          if (!seg.kind) style.background = tone(i, parts.length);
          return (
            <span
              key={seg.label + i}
              className={"fd-bar__seg" + (seg.kind ? " fd-bar__seg--" + seg.kind : "")}
              style={style}
              title={`${seg.label} ${format(seg.value)}`}
            />
          );
        })}
      </div>

      {segments ? (
        <ul className="fd-bar__legend">
          {segments.map((seg, i) => (
            <li key={seg.label + i} className="fd-bar__legenditem">
              <span
                className={
                  "fd-bar__swatch" + (seg.kind ? " fd-bar__swatch--" + seg.kind : "")
                }
                style={seg.kind ? undefined : { background: tone(i, segments.length) }}
                aria-hidden="true"
              />
              {seg.label}
              <span className="fd-bar__legendvalue">{format(seg.value)}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
