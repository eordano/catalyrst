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

export type LedgerResults = Record<string, Datum<unknown> | undefined>;

function toRow(entry: SourceEntry, probed: Datum<unknown> | undefined): SourceLedgerRow {
  const note = entry.today ? `${entry.note} Today: ${entry.today}` : entry.note;
  return {
    id: entry.id,
    datum: entry.datum,
    endpoint: entry.endpoint,
    usedBy: entry.usedBy,
    note,
    probed: probed ?? null,
  };
}

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

export function screenLedger(opts: {
  usedBy: readonly string[];
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
