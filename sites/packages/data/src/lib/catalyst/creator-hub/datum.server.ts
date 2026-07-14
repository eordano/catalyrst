/**
 * The server-side constructors for the `Datum` vocabulary.
 *
 * `Datum` itself and its six raw constructors are owned by ui3
 * (`catalyrst/ui3/src/creatorhub/lib/datum.ts`) -- see BUILD SPEC S5. This module is the
 * ONLY file in `@data` that imports them, so it is the single integration point
 * if those signatures move. It exists so that no loader in this package ever
 * writes a `Datum` object literal, and so that `reason` strings are *derived*
 * from the failure rather than authored per call site.
 *
 * The constructors are called positionally in the field order of the union
 * declared in the spec:
 *   live(value, endpoint, readAt)
 *   sampled(value, endpoint, readAt, takenAt, cadenceSeconds)
 *   snapshot(value, endpoint, readAt, exportedAt, exportSource)
 *   noSample(endpoint, takenAt, note)
 *   unavailable(endpoint, status, reason)
 *   unbuilt(subject, reason, today)
 */
import {
  DEFAULT_CADENCE_SECONDS,
  live,
  noSample as noSampleDatum,
  sampled,
  showable,
  snapshot,
  unavailable,
  unbuilt,
  NO_VALUE,
  type Datum,
} from "@ui/creatorhub/lib/datum";

import { CatalystError } from "../client";

export type { Datum } from "@ui/creatorhub/lib/datum";
export { DEFAULT_CADENCE_SECONDS, showable, NO_VALUE };

function nowIso(): string {
  return new Date().toISOString();
}

/**
 * `GET host/path` -- the mono line `DatumNote` renders and the string every
 * `reason` is built around. Scheme stripped; query kept, because a truncated
 * query would misreport what was asked for.
 */
export function endpointLabel(method: string, url: string): string {
  return `${method} ${url.replace(/^https?:\/\//, "")}`;
}

/**
 * When a showable datum was measured. For `sampled` that is the sample's own
 * `takenAt` -- never the read time, or a stopped sampler would look fresh.
 */
export function sampleTime<T>(
  d: Extract<Datum<T>, { value: T }>,
): string {
  if (d.state === "sampled") return d.takenAt;
  if (d.state === "snapshot") return d.exportedAt;
  return d.readAt;
}

/** The cadence a showable datum was sampled at, or null if it is not sampled. */
export function sampleCadence<T>(
  d: Extract<Datum<T>, { value: T }>,
): number | null {
  return d.state === "sampled" ? d.cadenceSeconds : null;
}

export function liveNow<T>(value: T, endpoint: string, readAt = nowIso()): Datum<T> {
  return live(value, endpoint, readAt);
}

export function sampledAt<T>(
  value: T,
  endpoint: string,
  takenAt: string,
  cadenceSeconds: number = DEFAULT_CADENCE_SECONDS,
  readAt = nowIso(),
): Datum<T> {
  return sampled(value, endpoint, takenAt, cadenceSeconds, readAt);
}

export function noSample(
  endpoint: string,
  takenAt: string,
  note: string,
): Datum<never> {
  return noSampleDatum(endpoint, takenAt, note);
}

/**
 * The only snapshot constructor a loader may reach. `snapshot()` itself throws
 * unless `exportSource === "metabase"` (spec S5.5 rule 5) -- a synthetic export
 * behind a warning chip is still a lie, so the artifact's own `source` field is
 * what decides, and a non-metabase artifact degrades to `unavailable` here.
 */
export function snapshotFrom<T>(
  value: T,
  endpoint: string,
  exportedAt: string,
  exportSource: string,
  readAt = nowIso(),
): Datum<T> {
  if (exportSource !== "metabase") {
    return unavailable(
      endpoint,
      null,
      `${endpoint} answered, but the artifact it serves reports source: ${JSON.stringify(
        exportSource,
      )} (not "metabase"), exported ${exportedAt}. No values are shown. Showing no value rather than a guess.`,
    );
  }
  return snapshot(value, endpoint, exportedAt, exportSource, readAt);
}

export function unbuiltDatum(
  subject: string,
  reason: string,
  today: string | null = null,
): Datum<never> {
  return unbuilt(subject, reason, today);
}

function statusOf(err: unknown): number | null {
  if (err instanceof CatalystError) return err.status > 0 ? err.status : null;
  return null;
}

function detailOf(err: unknown): string | null {
  if (err instanceof CatalystError && err.serverMessage) return err.message;
  if (err instanceof Error && err.name === "AbortError") return "the request timed out";
  return null;
}

/**
 * Derives an `unavailable` datum from a thrown read. Never invents a status and
 * never substitutes a value -- a `catch` block in this package can only ever
 * produce this state, which is what makes "no data" distinguishable from "zero".
 */
export function unavailableFrom(
  err: unknown,
  endpoint: string,
  hint?: string,
): Datum<never> {
  const status = statusOf(err);
  const detail = detailOf(err);
  const head =
    status === null
      ? `${endpoint} did not respond.`
      : `${endpoint} returned ${status}.`;
  const parts = [head];
  if (detail) parts.push(`${detail}.`);
  if (hint) parts.push(hint);
  parts.push("Showing no value rather than a guess.");
  return unavailable(endpoint, status, parts.join(" "));
}

/**
 * For a read that SUCCEEDED but whose payload may not be rendered -- a gated
 * endpoint, or an artifact that reports itself synthetic. Not a fault, so the
 * status is null and the reason carries the whole explanation.
 */
export function unavailableBecause(
  endpoint: string,
  reason: string,
): Datum<never> {
  return unavailable(
    endpoint,
    null,
    `${reason} Showing no value rather than a guess.`,
  );
}

/**
 * For a read that succeeded but whose subject was not supplied -- used by the
 * data-sources ledger, where a probe needs an address or a world name it was
 * not given. Not a failure, and deliberately not `unavailable`.
 */
export function notProbed(endpoint: string, need: string): Datum<never> {
  return noSampleDatum(
    endpoint,
    nowIso(),
    `Not probed: this endpoint needs ${need} and none was supplied on this request.`,
  );
}
