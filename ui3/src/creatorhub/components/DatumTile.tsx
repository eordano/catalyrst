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
  format?: (v: number | string) => string;
  unit?: string;
  note?: string;
  compare?: { label: string; datum: Datum<number> };
  now?: number;
};

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

export { NO_VALUE };
