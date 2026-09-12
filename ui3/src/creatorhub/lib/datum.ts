
export type DatumState =
  | "live"
  | "sampled"
  | "snapshot"
  | "no-sample"
  | "unavailable"
  | "unbuilt";

export type Datum<T> =
  | { state: "live"; value: T; endpoint: string; readAt: string }
  | {
      state: "sampled";
      value: T;
      endpoint: string;
      readAt: string;
      takenAt: string;
      cadenceSeconds: number;
    }
  | {
      state: "snapshot";
      value: T;
      endpoint: string;
      readAt: string;
      exportedAt: string;
      exportSource: string;
    }
  | { state: "no-sample"; endpoint: string; takenAt: string; note: string }
  | {
      state: "unavailable";
      endpoint: string;
      status: number | null;
      reason: string;
    }
  | { state: "unbuilt"; subject: string; reason: string; today: string | null };

export type ShowableDatum<T> = Extract<Datum<T>, { value: T }>;

export type NoSampleDatum = Extract<Datum<never>, { state: "no-sample" }>;
export type UnavailableDatum = Extract<Datum<never>, { state: "unavailable" }>;
export type UnbuiltDatum = Extract<Datum<never>, { state: "unbuilt" }>;

export const NO_VALUE = "\u{2014}";

export const DEFAULT_CADENCE_SECONDS = 300;

export const STALE_SAMPLE_FACTOR = 3;

export const STALE_SNAPSHOT_DAYS = 30;

export const TRUSTED_EXPORT_SOURCE = "metabase";

export const PUBLIC_DATA_DISCLOSURE =
  "Everything on this page is public. worlds-content-server, /presence/* and the Places API all answer unauthenticated requests. Your address selects which rows you see; it does not protect them.";

export const NO_ADDRESS_TITLE = "No address yet";
export const NO_ADDRESS_BODY =
  "This page needs an address to pick which worlds to show. It is not a login \u{2014} the data is public either way.";

export function showable<T>(d: Datum<T>): d is ShowableDatum<T> {
  return d.state === "live" || d.state === "sampled" || d.state === "snapshot";
}

export function isStale<T>(d: Datum<T>, now = Date.now()): boolean {
  if (d.state === "sampled")
    return (
      now - Date.parse(d.takenAt) > d.cadenceSeconds * STALE_SAMPLE_FACTOR * 1000
    );
  if (d.state === "snapshot")
    return now - Date.parse(d.exportedAt) > STALE_SNAPSHOT_DAYS * 86_400_000;
  return false;
}

const STATE_WORD: Record<DatumState, string> = {
  live: "Live",
  sampled: "Sampled",
  snapshot: "Snapshot",
  "no-sample": "No sample",
  unavailable: "Unavailable",
  unbuilt: "Not built",
};

export function stateWord<T>(d: Datum<T>, now?: number): string {
  if (isStale(d, now)) return "Stale";
  return STATE_WORD[d.state];
}

export const DATUM_GLYPH: Record<DatumState, string> = {
  live: "\u{25CF}",
  sampled: "\u{25D0}",
  snapshot: "\u{25D4}",
  "no-sample": "\u{25CC}",
  unavailable: "\u{2298}",
  unbuilt: "\u{25A8}",
};

export function datumGlyph<T>(d: Datum<T>): string {
  return DATUM_GLYPH[d.state];
}

export function datumModifier<T>(d: Datum<T>): string {
  switch (d.state) {
    case "no-sample":
      return "nosample";
    default:
      return d.state;
  }
}

export function datumEndpoint<T>(d: Datum<T>): string | null {
  return d.state === "unbuilt" ? null : d.endpoint;
}

export function datumTimestamp<T>(d: Datum<T>): string | null {
  switch (d.state) {
    case "live":
      return d.readAt;
    case "sampled":
      return d.takenAt;
    case "snapshot":
      return d.exportedAt;
    case "no-sample":
      return d.takenAt;
    default:
      return null;
  }
}

export function requiresNote<T>(d: Datum<T>): boolean {
  return (
    d.state === "no-sample" || d.state === "unavailable" || d.state === "unbuilt"
  );
}

export function formatDatum<T>(
  d: Datum<T>,
  format?: (value: T) => string,
): string {
  if (!showable(d)) return NO_VALUE;
  return format ? format(d.value) : String(d.value);
}

export function disagree<A, B>(a: Datum<A>, b: Datum<B>): boolean {
  if (!showable(a) || !showable(b)) return false;
  return !Object.is(a.value as unknown, b.value as unknown);
}

export type StateTally = {
  state: DatumState;
  stale: boolean;
  word: string;
  glyph: string;
  count: number;
};

export function tallyStates(
  datums: readonly Datum<unknown>[],
  now?: number,
): StateTally[] {
  const order: DatumState[] = [
    "live",
    "sampled",
    "snapshot",
    "no-sample",
    "unavailable",
    "unbuilt",
  ];
  const out: StateTally[] = [];
  for (const state of order) {
    for (const stale of [false, true]) {
      const count = datums.filter(
        (d) => d.state === state && isStale(d, now) === stale,
      ).length;
      if (count === 0) continue;
      out.push({
        state,
        stale,
        word: stale ? "Stale" : STATE_WORD[state],
        glyph: DATUM_GLYPH[state],
        count,
      });
    }
  }
  return out;
}

function ms(iso: string | null | undefined): number | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  return Number.isNaN(t) ? null : t;
}

export function formatUtcTime(iso: string | null | undefined): string | null {
  const t = ms(iso);
  if (t === null) return null;
  return `${new Date(t).toISOString().slice(11, 19)} UTC`;
}

export function formatUtcMinute(iso: string | null | undefined): string | null {
  const t = ms(iso);
  if (t === null) return null;
  return `${new Date(t).toISOString().slice(11, 16)} UTC`;
}

export function formatUtcDay(iso: string | null | undefined): string | null {
  const t = ms(iso);
  if (t === null) return null;
  return new Intl.DateTimeFormat("en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(t));
}

export function relativeAge(
  iso: string | null | undefined,
  now = Date.now(),
): string | null {
  const t = ms(iso);
  if (t === null) return null;
  const delta = now - t;
  if (delta < 0) return "just now";
  const minutes = Math.floor(delta / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

export function formatReadStamp(
  iso: string | null | undefined,
  now = Date.now(),
): string | null {
  const absolute = formatUtcTime(iso);
  if (absolute === null) return null;
  const rel = relativeAge(iso, now);
  return rel ? `${absolute} (${rel})` : absolute;
}

export function badgeText<T>(d: Datum<T>, now?: number): string {
  const word = stateWord(d, now);
  if (d.state === "sampled") {
    const rel = relativeAge(d.takenAt, now);
    return rel ? `${word} \u{B7} ${rel}` : word;
  }
  if (d.state === "snapshot") {
    const day = formatUtcDay(d.exportedAt);
    return day ? `${word} \u{B7} exported ${day}` : word;
  }
  return word;
}

const GUESS_SENTENCE = "Showing no value rather than a guess.";

export function noteLines<T>(d: Datum<T>, now?: number): string[] {
  switch (d.state) {
    case "live": {
      const at = formatUtcTime(d.readAt);
      return [d.endpoint, at ? `read at ${at}` : "read time unknown"];
    }
    case "sampled": {
      const at = formatUtcTime(d.takenAt);
      const minutes = Math.round(d.cadenceSeconds / 60);
      const lines = [
        d.endpoint,
        `sampled every ${minutes} min`,
        at ? `taken at ${at}` : "sample time unknown",
      ];
      if (isStale(d, now))
        lines.push(
          `The last sample is older than ${STALE_SAMPLE_FACTOR}\u{D7} the ${minutes}-minute cadence \u{2014} the sampler may have stopped.`,
        );
      return lines;
    }
    case "snapshot": {
      const lines = [
        d.endpoint,
        `exported ${d.exportedAt} from ${d.exportSource}. Not live.`,
      ];
      if (isStale(d, now))
        lines.push(
          `This export is more than ${STALE_SNAPSHOT_DAYS} days old.`,
        );
      return lines;
    }
    case "no-sample":
      return [d.endpoint, d.note];
    case "unavailable": {
      const reason = d.reason.trim();
      return [
        d.endpoint,
        reason.includes(GUESS_SENTENCE) ? reason : `${reason} ${GUESS_SENTENCE}`,
      ];
    }
    case "unbuilt":
      return d.today === null
        ? [d.reason]
        : [d.reason, `Today: ${d.today}`];
  }
}

export function live<T>(
  value: T,
  endpoint: string,
  readAt: string = new Date().toISOString(),
): Datum<T> {
  return { state: "live", value, endpoint, readAt };
}

export function sampled<T>(
  value: T,
  endpoint: string,
  takenAt: string,
  cadenceSeconds: number = DEFAULT_CADENCE_SECONDS,
  readAt: string = new Date().toISOString(),
): Datum<T> {
  return {
    state: "sampled",
    value,
    endpoint,
    readAt,
    takenAt,
    cadenceSeconds,
  };
}

export function snapshot<T>(
  value: T,
  endpoint: string,
  exportedAt: string,
  exportSource: string,
  readAt: string = new Date().toISOString(),
): Datum<T> {
  if (exportSource !== TRUSTED_EXPORT_SOURCE)
    throw new Error(
      `snapshot(): exportSource must be "${TRUSTED_EXPORT_SOURCE}", got "${exportSource}". ` +
        "A synthetic export is not a reading \u{2014} build an unavailable() datum instead.",
    );
  return { state: "snapshot", value, endpoint, readAt, exportedAt, exportSource };
}

export function noSample(
  endpoint: string,
  takenAt: string,
  note: string,
): NoSampleDatum {
  return { state: "no-sample", endpoint, takenAt, note };
}

export function unavailable(
  endpoint: string,
  status: number | null,
  reason: string,
): UnavailableDatum {
  return { state: "unavailable", endpoint, status, reason };
}

export function unbuilt(
  subject: string,
  reason: string,
  today: string | null = null,
): UnbuiltDatum {
  return { state: "unbuilt", subject, reason, today };
}
