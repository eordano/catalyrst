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

export const DERIVED_LABELS = {
  peak: "Peak concurrent (sampled)",
  occupied: "Snapshots with someone in it",
  firstSeen: "History begins",
} as const;

export const HISTORY_MIN_LIMIT = 1;
export const HISTORY_MAX_LIMIT = 5000;
export const HISTORY_DEFAULT_LIMIT = 200;

export function clampHistoryLimit(limit: number | null | undefined): number {
  const n = Number(limit);
  if (!Number.isFinite(n)) return HISTORY_DEFAULT_LIMIT;
  return Math.min(HISTORY_MAX_LIMIT, Math.max(HISTORY_MIN_LIMIT, Math.trunc(n)));
}

const takenAt = z.string();

export const WorldOccupancyRowSchema = WorldHeadcountRowSchema.extend({
  taken_at: takenAt,
});
export type WorldOccupancyRow = z.infer<typeof WorldOccupancyRowSchema>;

export const SceneHistoryRowSchema = SceneOccupancyRowSchema.extend({
  taken_at: takenAt,
});
export type SceneHistoryRow = z.infer<typeof SceneHistoryRowSchema>;

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

export type OccupancyPoint = { date: string; value: number | null };
export type GapBand = { fromIndex: number; toIndex: number };

export type BucketizedHistory = {
  points: OccupancyPoint[];
  gapBands: GapBand[];
  sampleCount: number;
  occupiedCount: number;
  firstSeen: string | null;
  lastSeen: string | null;
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

export function occupiedLabel(h: BucketizedHistory): string {
  return `${h.occupiedCount.toLocaleString()} of ${h.sampleCount.toLocaleString()}`;
}
