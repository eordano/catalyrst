import { DATUM_GLYPH, type Datum } from "../lib/datum";
import DatumBadge from "./DatumBadge";
import "./sourceledger.css";

export type SourceClass =
  | "live"
  | "sampled"
  | "snapshot"
  | "unavailable"
  | "unbuilt"
  | "excluded";

export type SourceLedgerRow = {
  id: string;
  datum: string;
  endpoint: string;
  usedBy: string[];
  note: string;
  probed?: Datum<unknown> | null;
};

export type SourceLedgerGroup = {
  klass: SourceClass;
  label: string;
  note?: string;
  rows: SourceLedgerRow[];
  emptyNote?: string;
};

export type SourceLedgerProps = {
  groups: readonly SourceLedgerGroup[];
  headingLevel?: 2 | 3 | 4;
  now?: number;
};

const CLASS_GLYPH: Record<SourceClass, string> = {
  live: DATUM_GLYPH.live,
  sampled: DATUM_GLYPH.sampled,
  snapshot: DATUM_GLYPH.snapshot,
  unavailable: DATUM_GLYPH.unavailable,
  unbuilt: DATUM_GLYPH.unbuilt,
  excluded: "\u{2715}",
};

const CLASS_WORD: Record<SourceClass, string> = {
  live: "Live",
  sampled: "Sampled",
  snapshot: "Snapshot",
  unavailable: "Unavailable",
  unbuilt: "Not built",
  excluded: "Excluded",
};

function ClassChip({ klass }: { klass: SourceClass }) {
  return (
    <span className={`dv-badge dv-badge--${klass === "excluded" ? "unbuilt" : klass}`}>
      <span className="dv-badge__glyph" aria-hidden="true">
        {CLASS_GLYPH[klass]}
      </span>
      <span className="dv-badge__word">{CLASS_WORD[klass]}</span>
    </span>
  );
}

export default function SourceLedger({
  groups,
  headingLevel = 3,
  now,
}: SourceLedgerProps) {
  const Heading = `h${headingLevel}` as "h2" | "h3" | "h4";
  return (
    <div className="sl">
      {groups.map((group) => (
        <section
          className={`sl__group sl__group--${group.klass}`}
          key={group.klass + group.label}
          aria-labelledby={`sl-h-${group.klass}`}
        >
          <Heading className="sl__grouphead" id={`sl-h-${group.klass}`}>
            <span className="sl__groupglyph" aria-hidden="true">
              {CLASS_GLYPH[group.klass]}
            </span>
            <span className="sl__grouplabel">{group.label}</span>
            <span className="sl__groupcount">{group.rows.length}</span>
          </Heading>

          {group.note ? <p className="sl__groupnote">{group.note}</p> : null}

          {group.rows.length === 0 ? (
            <p className="sl__empty">
              {group.emptyNote ??
                "No sources in this group."}
            </p>
          ) : (
            <div className="sl__scroll" tabIndex={0}>
              <table className="sl__table">
                <thead>
                  <tr>
                    <th scope="col">Datum</th>
                    <th scope="col">Endpoint</th>
                    <th scope="col">State</th>
                    <th scope="col">Used by</th>
                  </tr>
                </thead>
                <tbody>
                  {group.rows.map((row) => (
                    <tr key={row.id}>
                      <th scope="row" className="sl__datum">
                        <span className="sl__datumname">{row.datum}</span>
                        <span className="sl__note">{row.note}</span>
                      </th>
                      <td className="sl__endpoint">{row.endpoint}</td>
                      <td className="sl__state">
                        {row.probed ? (
                          <DatumBadge datum={row.probed} now={now} />
                        ) : (
                          <ClassChip klass={group.klass} />
                        )}
                        {row.probed ? (
                          <span className="sl__probed">probed just now</span>
                        ) : null}
                      </td>
                      <td className="sl__usedby">
                        {row.usedBy.length === 0 ? (
                          <span className="sl__nouse">nothing yet</span>
                        ) : (
                          row.usedBy.join(" \u{B7} ")
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      ))}
    </div>
  );
}
