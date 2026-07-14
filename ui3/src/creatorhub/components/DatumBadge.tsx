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
  /** Pinned clock, for deterministic stories and tests. */
  now?: number;
};

/**
 * The only thing in the tree that renders a state word. Glyph plus word,
 * always -- colour never carries the meaning on its own, so the state survives
 * greyscale, and the glyph is `aria-hidden` so a screen reader hears the word.
 */
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
  /** Built with `tallyStates()` -- the page owns which datums it counts. */
  tally: readonly StateTally[];
};

/**
 * The header strip: `* Live 2 -  Sampled 2 -  Unavailable 1`. It is also the
 * legend the em dash needs, so `--` is never unexplained on a screen.
 * States with no readings are omitted rather than shown as a zero.
 */
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
