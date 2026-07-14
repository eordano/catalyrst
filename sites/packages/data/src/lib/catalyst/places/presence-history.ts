import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import {
  PRESENCE_BASE,
  SceneOccupancyRowSchema,
  WorldHeadcountRowSchema,
} from "./presence";

export {
  PRESENCE_BASE,
  parsePointer,
  sceneJumpUrl,
  worldJumpUrl,
} from "./presence";

/*
 * WHAT THIS MODULE MAY NOT DERIVE -- do not add, do not "just estimate":
 *
 *   unique visitors - sessions - session length - dwell time - returning share
 *   - DAU/MAU - retention curves - device split
 *
 * `catalyrst-presence` stores player addresses internally
 * (`scene_occupancy.addresses`, `world_membership.addresses`) but its HTTP API
 * returns COUNTS ONLY. None of the above is derivable from a count series, and
 * a plausible-looking estimate is exactly the failure this feature exists to
 * prevent. The three derived numbers below are the only ones allowed, and they
 * carry mandated labels (see DERIVED_LABELS).
 */
export const DERIVED_LABELS = {
  peak: "Peak concurrent (sampled)",
  occupied: "Snapshots with someone in it",
  firstSeen: "History begins",
} as const;

/** `catalyrst-presence` clamps to this server-side; we clamp before asking so
 *  the query string never lies about what was requested. */
export const HISTORY_MIN_LIMIT = 1;
export const HISTORY_MAX_LIMIT = 5000;
export const HISTORY_DEFAULT_LIMIT = 200;

export function clampHistoryLimit(limit: number | null | undefined): number {
  const n = Number(limit);
  if (!Number.isFinite(n)) return HISTORY_DEFAULT_LIMIT;
  return Math.min(HISTORY_MAX_LIMIT, Math.max(HISTORY_MIN_LIMIT, Math.trunc(n)));
}

/** History rows are the `current/*` rows plus the `taken_at` of their snapshot.
 *  `taken_at` is required, not defaulted: a row with no timestamp cannot be
 *  bucketed, aged, or told apart from a stopped sampler, so it is dropped by
 *  the per-row loops below rather than dated to the epoch. */
const takenAt = z.string();

export const WorldOccupancyRowSchema = WorldHeadcountRowSchema.extend({
  taken_at: takenAt,
});
export type WorldOccupancyRow = z.infer<typeof WorldOccupancyRowSchema>;

export const SceneHistoryRowSchema = SceneOccupancyRowSchema.extend({
  taken_at: takenAt,
});
export type SceneHistoryRow = z.infer<typeof SceneHistoryRowSchema>;

/*
 * These three envelopes are the sharpest case the honesty gate exists for. The
 * handler always sends the key, so a body without it is not this endpoint --
 * defaulting to `[]` would render "your world has no history" for an error
 * page. Each reader below throws instead, and every caller wraps the read in a
 * try/catch that answers `unavailable`.
 */
const HistoryEnvelopeSchema = z.object({
  history: z.array(z.unknown()),
});

function historyPath(suffix: string): string {
  return `${PRESENCE_BASE}${suffix}`;
}

export function worldHistoryPath(): string {
  return historyPath("/worlds/history");
}

export function sceneHistoryPath(): string {
  return historyPath("/scenes/history");
}

/** `GET {catalyst}/presence/worlds/history?world={world}&limit={limit}` */
export async function fetchWorldHistory(
  world: string,
  limit: number,
  opts: GetOptions = {},
): Promise<WorldOccupancyRow[]> {
  const raw = await getJSON<unknown>(worldHistoryPath(), {
    ...opts,
    query: { world, limit: clampHistoryLimit(limit) },
  });
  const env = HistoryEnvelopeSchema.safeParse(raw);
  if (!env.success) {
    throw new Error("presence /worlds/history did not return a history array");
  }
  const out: WorldOccupancyRow[] = [];
  for (const r of env.data.history) {
    const parsed = WorldOccupancyRowSchema.safeParse(r);
    if (parsed.success) out.push(parsed.data);
  }
  return out;
}

/** `GET {catalyst}/presence/scenes/history?pointer={x,y}&limit={limit}` */
export async function fetchSceneHistory(
  pointer: string,
  limit: number,
  opts: GetOptions = {},
): Promise<SceneHistoryRow[]> {
  const raw = await getJSON<unknown>(sceneHistoryPath(), {
    ...opts,
    query: { pointer, limit: clampHistoryLimit(limit) },
  });
  const env = HistoryEnvelopeSchema.safeParse(raw);
  if (!env.success) {
    throw new Error("presence /scenes/history did not return a history array");
  }
  const out: SceneHistoryRow[] = [];
  for (const r of env.data.history) {
    const parsed = SceneHistoryRowSchema.safeParse(r);
    if (parsed.success) out.push(parsed.data);
  }
  return out;
}

const CurrentWorldsEnvelopeSchema = z.object({
  worlds: z.array(z.unknown()),
});
const CurrentScenesEnvelopeSchema = z.object({
  scenes: z.array(z.unknown()),
});

/*
 * `presence.ts` already fetches `/presence/current/worlds` and
 * `/presence/current/scenes`, but its row schemas predate this feature and drop
 * `taken_at` -- and without a `taken_at` there is no honest way to build a
 * `sampled` datum or to tell a fresh sample from a stopped sampler. These two
 * read the same endpoints with the same row schemas *plus* that field. They are
 * not a fork of the parsing rules: `WorldOccupancyRowSchema` and
 * `SceneHistoryRowSchema` extend the originals.
 */
export async function fetchCurrentWorldRows(
  opts: GetOptions = {},
): Promise<WorldOccupancyRow[]> {
  const raw = await getJSON<unknown>(historyPath("/current/worlds"), opts);
  const env = CurrentWorldsEnvelopeSchema.safeParse(raw);
  if (!env.success) {
    throw new Error("presence /current/worlds did not return a worlds array");
  }
  const out: WorldOccupancyRow[] = [];
  for (const r of env.data.worlds) {
    const parsed = WorldOccupancyRowSchema.safeParse(r);
    if (parsed.success) out.push(parsed.data);
  }
  return out;
}

export async function fetchCurrentSceneRows(
  opts: GetOptions = {},
): Promise<SceneHistoryRow[]> {
  const raw = await getJSON<unknown>(historyPath("/current/scenes"), opts);
  const env = CurrentScenesEnvelopeSchema.safeParse(raw);
  if (!env.success) {
    throw new Error("presence /current/scenes did not return a scenes array");
  }
  const out: SceneHistoryRow[] = [];
  for (const r of env.data.scenes) {
    const parsed = SceneHistoryRowSchema.safeParse(r);
    if (parsed.success) out.push(parsed.data);
  }
  return out;
}

export function currentWorldsPath(): string {
  return historyPath("/current/worlds");
}

export function currentScenesPath(): string {
  return historyPath("/current/scenes");
}

export function currentPath(): string {
  return historyPath("/current");
}

/** Mirrors `DailyPoint` in `catalyrst/ui3/src/creatorhub/lib/scene-analytics.ts:76`.
 *  `AnalyticsChart` already breaks its path on `value: null`. */
export type OccupancyPoint = { date: string; value: number | null };
export type GapBand = { fromIndex: number; toIndex: number };

export type BucketizedHistory = {
  points: OccupancyPoint[];
  gapBands: GapBand[];
  /** buckets that carry a real sample */
  sampleCount: number;
  /** samples whose count was > 0 */
  occupiedCount: number;
  firstSeen: string | null;
  lastSeen: string | null;
  /** null when there is no sample at all -- a peak of 0 would read as "nobody
   *  ever came", which is a different fact from "we never sampled". */
  peak: number | null;
  cadenceSeconds: number;
};

export const EMPTY_HISTORY: BucketizedHistory = {
  points: [],
  gapBands: [],
  sampleCount: 0,
  occupiedCount: 0,
  firstSeen: null,
  lastSeen: null,
  peak: null,
  cadenceSeconds: 0,
};

type CountedRow = { taken_at: string; count: number };

/**
 * Turns a presence history into a plottable series.
 *
 * This function is where "a gap is not a zero" is implemented. Every cadence
 * bucket between the first and last sample that has no row becomes
 * `{ date, value: null }`, and contiguous null runs become `gapBands`.
 *
 * It never `?? 0`s a missing bucket, never interpolates across one, and never
 * extends the series past `lastSeen`. Rows exist only for the instants a world
 * or scene was live when the sampler ran, so a bucket with no row means "not in
 * the poll set", not "nobody was there".
 */
export function bucketize(
  rows: CountedRow[],
  cadenceSeconds = 300,
): BucketizedHistory {
  const cadence = Math.max(1, Math.trunc(cadenceSeconds));
  const cadenceMs = cadence * 1000;

  const parsed: { t: number; count: number }[] = [];
  for (const r of rows) {
    const t = Date.parse(r.taken_at);
    if (!Number.isFinite(t)) continue;
    parsed.push({ t, count: r.count });
  }
  if (parsed.length === 0) return { ...EMPTY_HISTORY, cadenceSeconds: cadence };

  parsed.sort((a, b) => a.t - b.t);

  // One bucket per cadence slot. When two rows land in the same slot the later
  // one wins -- it is the more recent measurement, not a maximum of two.
  const byBucket = new Map<number, { t: number; count: number }>();
  for (const p of parsed) {
    const bucket = Math.floor(p.t / cadenceMs);
    const prev = byBucket.get(bucket);
    if (!prev || p.t >= prev.t) byBucket.set(bucket, p);
  }

  const firstBucket = Math.floor(parsed[0].t / cadenceMs);
  const lastBucket = Math.floor(parsed[parsed.length - 1].t / cadenceMs);

  const points: OccupancyPoint[] = [];
  const gapBands: GapBand[] = [];
  let sampleCount = 0;
  let occupiedCount = 0;
  let peak: number | null = null;
  let runStart = -1;

  for (let b = firstBucket; b <= lastBucket; b++) {
    const hit = byBucket.get(b);
    const date = new Date(b * cadenceMs).toISOString();
    const index = points.length;
    if (hit) {
      points.push({ date, value: hit.count });
      sampleCount += 1;
      if (hit.count > 0) occupiedCount += 1;
      peak = peak === null ? hit.count : Math.max(peak, hit.count);
      if (runStart >= 0) {
        gapBands.push({ fromIndex: runStart, toIndex: index - 1 });
        runStart = -1;
      }
    } else {
      points.push({ date, value: null });
      if (runStart < 0) runStart = index;
    }
  }
  // A trailing run cannot happen (the last bucket always carries a sample), but
  // close it rather than rely on that.
  if (runStart >= 0) {
    gapBands.push({ fromIndex: runStart, toIndex: points.length - 1 });
  }

  return {
    points,
    gapBands,
    sampleCount,
    occupiedCount,
    firstSeen: new Date(parsed[0].t).toISOString(),
    lastSeen: new Date(parsed[parsed.length - 1].t).toISOString(),
    peak,
    cadenceSeconds: cadence,
  };
}

/** "Snapshots with someone in it -- 38 of 1 214". Never relabel this "visits". */
export function occupiedLabel(h: BucketizedHistory): string {
  return `${h.occupiedCount.toLocaleString()} of ${h.sampleCount.toLocaleString()}`;
}
