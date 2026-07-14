/**
 * The provenance vocabulary for the creator hub.
 *
 * A `Datum<T>` is a value *and* the story of where it came from. The union is
 * deliberately shaped so that the non-showable states have **no `value` key at
 * all**: `d.value ?? 0` does not type-check on an `unavailable` datum, so there
 * is no ergonomic way to launder a missing reading into a plausible number.
 * Do not "simplify" this to `{ value: T | null; state }` -- the absence of the
 * field *is* the enforcement mechanism.
 *
 * No CSS, no React, no imports outside ui3.
 */

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

/** The three variants that carry a value. */
export type ShowableDatum<T> = Extract<Datum<T>, { value: T }>;

/**
 * The three value-less variants. Each is assignable to `Datum<T>` for every
 * `T`, so a constructor can return one without knowing the value type.
 */
export type NoSampleDatum = Extract<Datum<never>, { state: "no-sample" }>;
export type UnavailableDatum = Extract<Datum<never>, { state: "unavailable" }>;
export type UnbuiltDatum = Extract<Datum<never>, { state: "unbuilt" }>;

/** Identical glyph to `NO_DATA` in scene-analytics.ts -- one em dash, everywhere. */
export const NO_VALUE = "\u{2014}";

/** `PRESENCE_SNAPSHOT_INTERVAL_SECS` on catalyrst-presence. */
export const DEFAULT_CADENCE_SECONDS = 300;

/** A sampled reading is stale once it is older than this many cadences. */
export const STALE_SAMPLE_FACTOR = 3;

export const STALE_SNAPSHOT_DAYS = 30;

/** The only `exportSource` whose numbers may ever be rendered. See rule 5. */
export const TRUSTED_EXPORT_SOURCE = "metabase";

/**
 * Standing disclosure, shown in every creator-hub activity header -- not a
 * dismissible banner. This is the honest handling of "no route in this app
 * rejects an unauthenticated request": the address scopes rows, it does not
 * protect them.
 */
export const PUBLIC_DATA_DISCLOSURE =
  "Everything on this page is public. worlds-content-server, /presence/* and the Places API all answer unauthenticated requests. Your address selects which rows you see; it does not protect them.";

/** The no-address state is scoping, not a sign-in wall. */
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

/**
 * Glyph is decorative -- it is always `aria-hidden` and always accompanied by
 * the word from `stateWord`, so state survives greyscale and screen readers.
 */
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

/** BEM modifier suffix for the badge/panel skins. `stale` skins sampled+snapshot. */
export function datumModifier<T>(d: Datum<T>): string {
  switch (d.state) {
    case "no-sample":
      return "nosample";
    default:
      return d.state;
  }
}

/** The endpoint this datum came from, or `null` for `unbuilt` (there is none). */
export function datumEndpoint<T>(d: Datum<T>): string | null {
  return d.state === "unbuilt" ? null : d.endpoint;
}

/** The timestamp that matters for this state: taken / exported / read. */
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

/**
 * `DatumNote` is mandatory beneath a tile in these states -- the reason a value
 * is missing is not optional chrome.
 */
export function requiresNote<T>(d: Datum<T>): boolean {
  return (
    d.state === "no-sample" || d.state === "unavailable" || d.state === "unbuilt"
  );
}

/**
 * Render a datum's value through `format`, or `NO_VALUE` when there is no
 * value to render. This is the only sanctioned way to turn a `Datum` into a
 * string -- it cannot fall back to `0` because there is nothing to fall back to.
 */
export function formatDatum<T>(
  d: Datum<T>,
  format?: (value: T) => string,
): string {
  if (!showable(d)) return NO_VALUE;
  return format ? format(d.value) : String(d.value);
}

/** True when both readings exist and disagree -- the side-by-side case. */
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

/**
 * Count the states present on a screen, for the header strip. Stale readings
 * are tallied separately from fresh ones -- a page of stale samples should not
 * read as a page of samples.
 */
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

// Time formatting -- absolute *and* relative, always

function ms(iso: string | null | undefined): number | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  return Number.isNaN(t) ? null : t;
}

/** "14:03:22 UTC" -- null when the timestamp is unparseable. */
export function formatUtcTime(iso: string | null | undefined): string | null {
  const t = ms(iso);
  if (t === null) return null;
  return `${new Date(t).toISOString().slice(11, 19)} UTC`;
}

/** "14:03 UTC" */
export function formatUtcMinute(iso: string | null | undefined): string | null {
  const t = ms(iso);
  if (t === null) return null;
  return `${new Date(t).toISOString().slice(11, 16)} UTC`;
}

/** "10 Jul 2026" in UTC. */
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

/** "just now" - "2m ago" - "3h ago" - "5d ago" - null when unparseable. */
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

/** "14:03 UTC (2m ago)" -- the house format. Absolute and relative, never one. */
export function formatReadStamp(
  iso: string | null | undefined,
  now = Date.now(),
): string | null {
  const absolute = formatUtcTime(iso);
  if (absolute === null) return null;
  const rel = relativeAge(iso, now);
  return rel ? `${absolute} (${rel})` : absolute;
}

/**
 * The badge text: word plus, for time-bearing states, its age.
 * `Sampled - 2m ago` - `Stale - 22m ago` - `Snapshot - exported 10 Jul 2026`.
 */
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

/**
 * The note lines for a datum, one sentence per line. Every string a screen
 * says about provenance is derived here, so the copy cannot drift between
 * surfaces.
 */
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

// Constructors -- the only sanctioned way to build a Datum

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

/**
 * Rule 5: a snapshot whose export is synthetic must never render its numbers.
 * `snapshot()` throws rather than returning a renderable datum, so a fixture
 * artifact cannot reach a tile even behind a warning chip.
 */
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
