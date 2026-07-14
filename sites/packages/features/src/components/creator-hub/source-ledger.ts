import {
  SOURCE_GROUP_NOTES,
  SOURCE_GROUP_ORDER,
  SOURCE_REGISTRY,
  type SourceClass,
  type SourceEntry,
} from "@data/lib/catalyst/creator-hub/data-sources";

import type {
  SourceLedgerGroup,
  SourceLedgerRow,
} from "@ui/creatorhub/components/SourceLedger";
import type { Datum } from "@ui/creatorhub/lib/datum";

/*
 * Turns the source registry plus whatever this request actually read into the
 * groups `SourceLedger` renders.
 *
 * The one rule that matters: `probed` is attached ONLY for a row this request
 * genuinely read. `SourceLedger` renders a live `DatumBadge` plus "probed just
 * now" when it is present, and a static class chip when it is not -- so handing
 * it a result the loader did not obtain would make the ledger claim a check it
 * never performed. That is the exact failure this whole feature exists to
 * prevent, so the mapping is by id and nothing is defaulted.
 */

export const SOURCE_GROUP_LABELS: Record<SourceClass, string> = {
  live: "Live",
  sampled: "Sampled",
  snapshot: "Snapshot",
  unavailable: "Unavailable",
  unbuilt: "Not built",
  excluded: "Excluded on purpose",
};

const EMPTY_NOTES: Partial<Record<SourceClass, string>> = {
  snapshot:
    "The vocabulary exists and nothing currently qualifies. A snapshot is a dated export from metabase; an artifact reporting any other source is not rendered at all.",
};

/** `Datum`s keyed by `SourceEntry.id`, for rows this request really read. */
export type LedgerResults = Record<string, Datum<unknown> | undefined>;

function toRow(entry: SourceEntry, probed: Datum<unknown> | undefined): SourceLedgerRow {
  const note = entry.today ? `${entry.note} Today: ${entry.today}` : entry.note;
  return {
    id: entry.id,
    datum: entry.datum,
    endpoint: entry.endpoint,
    usedBy: entry.usedBy,
    note,
    // `?? null` rather than the raw value: an explicit null keeps the static
    // class chip, which is what an unprobed row must show.
    probed: probed ?? null,
  };
}

/**
 * Every registry group in class order, including the empty ones -- the page
 * renders what it is given and counts what it renders.
 */
export function buildLedgerGroups(
  entries: readonly SourceEntry[],
  results: LedgerResults = {},
): SourceLedgerGroup[] {
  return SOURCE_GROUP_ORDER.map((klass) => {
    const rows = entries
      .filter((e) => e.klass === klass)
      .map((e) => toRow(e, results[e.id]));
    const group: SourceLedgerGroup = {
      klass,
      label: SOURCE_GROUP_LABELS[klass],
      rows,
    };
    const note = SOURCE_GROUP_NOTES[klass];
    if (note) group.note = note;
    const emptyNote = EMPTY_NOTES[klass];
    if (emptyNote) group.emptyNote = emptyNote;
    return group;
  });
}

/**
 * The per-screen footer ledger: the endpoints this screen touched, plus the
 * ones it deliberately did not display, named by id so the omissions are as
 * legible as the reads.
 */
export function screenLedger(opts: {
  /** Registry `usedBy` values that select this screen's own reads. */
  usedBy: readonly string[];
  /** Extra rows to carry, by id -- the "we read this and chose not to show it". */
  alsoIds?: readonly string[];
  results?: LedgerResults;
}): SourceLedgerGroup[] {
  const also = new Set(opts.alsoIds ?? []);
  const entries = SOURCE_REGISTRY.filter(
    (e) => also.has(e.id) || e.usedBy.some((u) => opts.usedBy.includes(u)),
  );
  return buildLedgerGroups(entries, opts.results ?? {}).filter(
    (g) => g.rows.length > 0,
  );
}
