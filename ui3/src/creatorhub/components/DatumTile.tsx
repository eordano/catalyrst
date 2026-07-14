import {
  NO_VALUE,
  datumModifier,
  disagree,
  formatDatum,
  isStale,
  showable,
  type Datum,
} from "../lib/datum";
import DatumBadge from "./DatumBadge";
import DatumNote from "./DatumNote";
import "./datumtile.css";

export type DatumTileProps = {
  label: string;
  datum: Datum<number | string>;
  /** Applied ONLY to showable states. There is nothing to format otherwise. */
  format?: (v: number | string) => string;
  unit?: string;
  /** Plain-language sentence. REQUIRED when the tile can render a literal 0. */
  note?: string;
  /**
   * A second measurement of the same thing from a different host. Rendered
   * beneath, quietly, ONLY when both are showable AND the values differ --
   * two hosts agreeing is not news, and picking one would be a lie.
   */
  compare?: { label: string; datum: Datum<number> };
  /** Pinned clock, for deterministic stories and tests. */
  now?: number;
};

/**
 * The only way a number reaches a card. There is no `value` prop and no
 * `unavailable` boolean: a caller cannot pass a figure without also passing
 * where it came from.
 *
 * The tile never moves, hides or resizes because its state changed -- it swaps
 * its badge, and the value becomes `--` at the same size and weight.
 */
export default function DatumTile({
  label,
  datum,
  format,
  unit,
  note,
  compare,
  now,
}: DatumTileProps) {
  const canShow = showable(datum);
  const text = formatDatum(datum, format);
  const stale = isStale(datum, now);

  if (
    import.meta.env?.DEV &&
    canShow &&
    (datum.value === 0 || datum.value === "0") &&
    !note
  ) {
    // A bare 0 is the most confusable number in this product: a real zero and
    // an absent reading look identical. S5.4 makes the note mandatory here.
    console.warn(
      `DatumTile "${label}" renders a literal 0 without a note. A real zero must say so ` +
        `(e.g. "a real zero \u{2014} sampled 2m ago, nobody in").`,
    );
  }

  const cls =
    "dt" +
    ` dt--${datumModifier(datum)}` +
    (stale ? " dt--stale" : "") +
    (canShow ? "" : " dt--absent");

  const showCompare = compare !== undefined && disagree(datum, compare.datum);

  return (
    <div className={cls}>
      <div className="dt__head">
        <span className="dt__label">{label}</span>
        <DatumBadge datum={datum} now={now} />
      </div>

      <p className="dt__value">
        <span className={canShow ? "dt__num" : "dt__num dt__num--absent"}>
          {text}
        </span>
        {canShow && unit ? <span className="dt__unit">{unit}</span> : null}
      </p>

      {note ? <p className="dt__note">{note}</p> : null}

      {showCompare && compare ? (
        <p className="dt__compare">
          <span className="dt__comparelabel">{compare.label}</span>
          <span className="dt__comparevalue">
            {formatDatum(compare.datum, (v) => `${v}`)}
          </span>
          <DatumBadge datum={compare.datum} now={now} />
        </p>
      ) : null}

      <DatumNote datum={datum} now={now} />
    </div>
  );
}

/** Re-exported so callers rendering a bare cell use the same glyph as the tile. */
export { NO_VALUE };
