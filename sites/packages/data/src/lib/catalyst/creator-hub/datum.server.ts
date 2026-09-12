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

export function endpointLabel(method: string, url: string): string {
  return `${method} ${url.replace(/^https?:\/\//, "")}`;
}

export function sampleTime<T>(
  d: Extract<Datum<T>, { value: T }>,
): string {
  if (d.state === "sampled") return d.takenAt;
  if (d.state === "snapshot") return d.exportedAt;
  return d.readAt;
}

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

export function notProbed(endpoint: string, need: string): Datum<never> {
  return noSampleDatum(
    endpoint,
    nowIso(),
    `Not probed: this endpoint needs ${need} and none was supplied on this request.`,
  );
}
