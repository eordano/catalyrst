import {
  badgeText,
  datumGlyph,
  datumModifier,
  isStale,
  type Datum,
  type StateTally,
} from "../lib/datum";
import "./datumbadge.css";

export type DatumBadgeProps = {
  datum: Datum<unknown>;
  now?: number;
};

export default function DatumBadge({ datum, now }: DatumBadgeProps) {
  const stale = isStale(datum, now);
  const cls =
    "dv-badge" +
    ` dv-badge--${datumModifier(datum)}` +
    (stale ? " dv-badge--stale" : "");

  return (
    <span className={cls}>
      <span className="dv-badge__glyph" aria-hidden="true">
        {datumGlyph(datum)}
      </span>
      <span className="dv-badge__word">{badgeText(datum, now)}</span>
    </span>
  );
}

export type DatumTallyProps = {
  tally: readonly StateTally[];
};

export function DatumTally({ tally }: DatumTallyProps) {
  if (tally.length === 0) return null;
  return (
    <p className="dv-tally">
      {tally.map((t) => (
        <span
          key={`${t.state}-${t.stale ? "stale" : "fresh"}`}
          className={
            "dv-badge dv-badge--" +
            (t.state === "no-sample" ? "nosample" : t.state) +
            (t.stale ? " dv-badge--stale" : "")
          }
        >
          <span className="dv-badge__glyph" aria-hidden="true">
            {t.glyph}
          </span>
          <span className="dv-badge__word">
            {t.word} {t.count}
          </span>
        </span>
      ))}
    </p>
  );
}
