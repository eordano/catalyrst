import { DATUM_GLYPH, type Datum } from "../lib/datum";
import DatumBadge from "./DatumBadge";
import "./sourceledger.css";

/**
 * Mirrors `SourceClass` in the data layer's source registry. `excluded` has no
 * `Datum` counterpart on purpose: an excluded source is one we can read and
 * refuse to render, which is a decision, not a state.
 */
export type SourceClass =
  | "live"
  | "sampled"
  | "snapshot"
  | "unavailable"
  | "unbuilt"
  | "excluded";

export type SourceLedgerRow = {
  id: string;
  /** The human name of the datum, e.g. "People in your worlds right now". */
  datum: string;
  /** `METHOD host/path`, or "--" for a row that has no endpoint (unbuilt). */
  endpoint: string;
  usedBy: string[];
  note: string;
  /**
   * The result of probing this row on this request. Present only for `live`
   * and `sampled` rows -- probing something that does not exist is theatre, and
   * a ledger that claims "live" without checking is decorative.
   */
  probed?: Datum<unknown> | null;
};

export type SourceLedgerGroup = {
  klass: SourceClass;
  label: string;
  /** Applies to the whole group, e.g. "A missing bucket is not a zero." */
  note?: string;
  rows: SourceLedgerRow[];
  /** Shown instead of the table when the group is legitimately empty. */
  emptyNote?: string;
};

export type SourceLedgerProps = {
  groups: readonly SourceLedgerGroup[];
  /**
   * Heading level for the group headers. `3` suits a ledger nested under an
   * `<h2>` section title; `2` suits the standalone Data sources page, where a
   * jump from `<h1>` to `<h3>` would be a heading-order violation.
   */
  headingLevel?: 2 | 3 | 4;
  /** Pinned clock, for deterministic stories and tests. */
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

/** Static state chip for rows that are constants and are never probed. */
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

/**
 * Every datum the hub can show, its state, and what reads it -- including the
 * endpoints we deliberately do not display, with the reason.
 *
 * Used both as the `/creator-hub/data-sources` table and as the per-screen
 * footer ledger, so a reader never has to take a screen's word for itself.
 */
/* `tabIndex={0}` on `.sl__scroll`: a horizontally scrollable region must be
   reachable by keyboard. It is a scroll container, not a control. */
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
