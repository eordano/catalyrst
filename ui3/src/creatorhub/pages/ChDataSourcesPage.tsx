import { useMemo } from "react";
import SourceLedger, {
  type SourceClass,
  type SourceLedgerGroup,
} from "../components/SourceLedger";
import { PUBLIC_DATA_DISCLOSURE, formatReadStamp } from "../lib/datum";
import "./chdatasources.css";

export type SourceFilter = SourceClass | "all";

export const SOURCE_FILTERS: { id: SourceFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "live", label: "Live" },
  { id: "sampled", label: "Sampled" },
  { id: "snapshot", label: "Snapshot" },
  { id: "unavailable", label: "Unavailable" },
  { id: "unbuilt", label: "Not built" },
  { id: "excluded", label: "Excluded" },
];

export type ChDataSourcesPageProps = {
  /**
   * Every group in class order, including the empty ones. The page renders
   * what it is given and counts what it renders -- it never invents a row.
   */
  groups: readonly SourceLedgerGroup[];
  /** ISO timestamp of this request. `null` renders as an unknown read time. */
  readAt?: string | null;
  filter?: SourceFilter;
  onFilterChange?: (filter: SourceFilter) => void;
  onRefresh?: () => void;
  refreshing?: boolean;
  /** Pinned clock, for deterministic stories and tests. */
  now?: number;
};

/**
 * The source ledger: every datum the hub can show, its state, probed live.
 *
 * `live` and `sampled` rows are probed on the request that renders this page,
 * so the ledger cannot claim "live" for something that is down. `unbuilt` and
 * `excluded` rows are constants and are never probed -- probing something that
 * does not exist is theatre.
 */
export default function ChDataSourcesPage({
  groups,
  readAt = null,
  filter = "all",
  onFilterChange,
  onRefresh,
  refreshing = false,
  now,
}: ChDataSourcesPageProps) {
  const shown = useMemo(
    () => (filter === "all" ? groups : groups.filter((g) => g.klass === filter)),
    [groups, filter],
  );

  const total = useMemo(
    () => groups.reduce((sum, g) => sum + g.rows.length, 0),
    [groups],
  );

  const stamp = formatReadStamp(readAt, now);

  return (
    <div className="ds">
      <header className="ds__head">
        <div className="ds__headmain">
          <h1 className="ds__title">Data sources</h1>
          <p className="ds__sub">
            Every datum this hub can show, the endpoint behind it, and whether
            that endpoint answered just now. {total} sources.
          </p>
        </div>

        <div className="ds__headside">
          <button
            type="button"
            className="ds__refresh"
            onClick={onRefresh}
            disabled={refreshing || !onRefresh}
          >
            <span aria-hidden="true">&#x27F3;</span>{" "}
            {refreshing ? "Re-probing\u{2026}" : "Re-probe"}
          </button>
          <p className="ds__stamp">
            {stamp ? `Read at ${stamp}` : "Read time unknown"}
          </p>
        </div>
      </header>

      <p className="ds__disclosure" role="note">
        {PUBLIC_DATA_DISCLOSURE}
      </p>

      <div className="ds__filters" role="group" aria-label="Filter by state">
        {SOURCE_FILTERS.map((f) => {
          const count =
            f.id === "all"
              ? total
              : (groups.find((g) => g.klass === f.id)?.rows.length ?? 0);
          const active = filter === f.id;
          return (
            <button
              key={f.id}
              type="button"
              className={active ? "ds__filter is-active" : "ds__filter"}
              aria-pressed={active}
              onClick={() => onFilterChange?.(f.id)}
              disabled={!onFilterChange}
            >
              {f.label} <span className="ds__filtercount">{count}</span>
            </button>
          );
        })}
      </div>

      {shown.length === 0 ? (
        <p className="ds__none">
          No source in this ledger is in that state right now.
        </p>
      ) : (
        <SourceLedger groups={shown} headingLevel={2} now={now} />
      )}

      <p className="ds__foot" role="note">
        &#x201C;Probed&#x201D; means this page asked the endpoint while rendering, with a
        four-second timeout. A row that is not probed is a constant: its state
        is a fact about what exists, not about what answered today.
      </p>
    </div>
  );
}
