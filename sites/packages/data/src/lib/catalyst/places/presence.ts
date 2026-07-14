import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import {
  CurrentSnapshotSchema,
  SceneOccupancyRowSchema,
  WorldHeadcountRowSchema,
} from "../generated-schemas/presence";

export const PRESENCE_BASE = "/presence";

/*
 * The row schemas are the generated images of `catalyrst-presence`'s DTOs
 * (ports/queries.rs), so every field Rust declares non-optional is required
 * here and a payload that is missing one FAILS the parse instead of arriving
 * as a zero. A snapshot without `peers_count`, or an occupancy row without a
 * `pointer`/`count`, is not a degraded reading -- it is not a reading, and the
 * callers below turn that into "unavailable"/"no sample" rather than into a
 * number nobody measured. Only the columns Rust declares `Option<_>`
 * (`worlds_live_total`, `scene_name`, `live_users`) are nullable. The hand
 * copy this replaces was wrong twice about the wire: `realm` is a required
 * `String` (never null) and rows carry a required `taken_at`; it also carried
 * a `parcel_count` column the endpoint never sends, which is gone.
 */
export { CurrentSnapshotSchema, SceneOccupancyRowSchema, WorldHeadcountRowSchema };
export type CurrentSnapshot = z.infer<typeof CurrentSnapshotSchema>;
export type SceneOccupancyRow = z.infer<typeof SceneOccupancyRowSchema>;
export type WorldHeadcountRow = z.infer<typeof WorldHeadcountRowSchema>;

/** `current: null` means the snapshot header was not read. `source` says which
 *  of the three reads failed at all, so a screen can suppress the numbers
 *  instead of drawing an unmeasured zero. */
export type PresenceSnapshot = {
  current: CurrentSnapshot | null;
  scenes: SceneOccupancyRow[] | null;
  worlds: WorldHeadcountRow[] | null;
  source: "catalyst" | "unavailable";
};

/* The `{current}`/`{scenes}`/`{worlds}` wrappers are ad-hoc `json!` envelopes
 * in the Rust handlers (no DTO, so nothing to generate); `current` is an
 * `Option<_>` there and arrives as an explicit null. */
const CurrentEnvelopeSchema = z.object({
  current: CurrentSnapshotSchema.nullable(),
});
const ScenesEnvelopeSchema = z.object({
  scenes: z.array(z.unknown()),
});
const WorldsEnvelopeSchema = z.object({
  worlds: z.array(z.unknown()),
});

function presencePath(suffix: string): string {
  return `${PRESENCE_BASE}${suffix}`;
}

export async function fetchCurrent(opts: GetOptions = {}): Promise<CurrentSnapshot | null> {
  const env = await getJSON<unknown>(presencePath("/current"), opts);
  const parsed = CurrentEnvelopeSchema.safeParse(env);
  if (!parsed.success || !parsed.data.current) return null;
  return parsed.data.current;
}

export async function fetchCurrentScenes(opts: GetOptions = {}): Promise<SceneOccupancyRow[]> {
  const env = await getJSON<unknown>(presencePath("/current/scenes"), opts);
  const parsed = ScenesEnvelopeSchema.safeParse(env);
  if (!parsed.success) {
    throw new Error("presence /current/scenes did not return a scenes array");
  }
  const raw = parsed.data.scenes;
  const out: SceneOccupancyRow[] = [];
  for (const r of raw) {
    const row = SceneOccupancyRowSchema.safeParse(r);
    if (row.success) out.push(row.data);
  }
  return out;
}

export async function fetchCurrentWorlds(opts: GetOptions = {}): Promise<WorldHeadcountRow[]> {
  const env = await getJSON<unknown>(presencePath("/current/worlds"), opts);
  const parsed = WorldsEnvelopeSchema.safeParse(env);
  if (!parsed.success) {
    throw new Error("presence /current/worlds did not return a worlds array");
  }
  const raw = parsed.data.worlds;
  const out: WorldHeadcountRow[] = [];
  for (const r of raw) {
    const row = WorldHeadcountRowSchema.safeParse(r);
    if (row.success) out.push(row.data);
  }
  return out;
}

/** Each read answers with its own rows or with `null`. A failed read must not
 *  be indistinguishable from an empty one, so it stays null and drags `source`
 *  down to "unavailable" for the whole snapshot. */
export async function fetchPresenceSnapshot(
  opts: GetOptions = {},
): Promise<PresenceSnapshot> {
  const [current, scenes, worlds] = await Promise.all([
    fetchCurrent(opts).catch(() => null),
    fetchCurrentScenes(opts).catch(() => null),
    fetchCurrentWorlds(opts).catch(() => null),
  ]);
  return {
    current,
    scenes,
    worlds,
    source:
      current === null || scenes === null || worlds === null
        ? "unavailable"
        : "catalyst",
  };
}

export function parsePointer(pointer: string): [number, number] {
  const [xs, ys] = (pointer || "0,0").split(",");
  const x = Number.parseInt((xs ?? "0").trim(), 10);
  const y = Number.parseInt((ys ?? "0").trim(), 10);
  return [Number.isFinite(x) ? x : 0, Number.isFinite(y) ? y : 0];
}

export function sceneJumpUrl(pointer: string): string {
  const pos = (pointer || "0,0").trim();
  return `https://catalyst.example.com/play/?position=${pos}`;
}

export function worldJumpUrl(worldName: string): string {
  return `https://catalyst.example.com/play/?realm=${encodeURIComponent(worldName)}`;
}

export type OccupancyTotals = {
  peers: number;
  scenes: number;
  worlds: number;
  sceneUsers: number;
  worldUsers: number;
  activeScenes: number;
  activeWorlds: number;
};

export function worldHeadcount(w: WorldHeadcountRow): number {
  return Math.max(w.count, w.live_users ?? 0);
}

/** Null when any of the three reads is missing: every figure below folds all
 *  three together, so a partial snapshot yields a total nobody measured. */
export function occupancyTotals(snap: PresenceSnapshot): OccupancyTotals | null {
  const c = snap.current;
  const scenes = snap.scenes;
  const worlds = snap.worlds;
  if (c === null || scenes === null || worlds === null) return null;
  const activeScenes = scenes.filter((s) => s.count > 0).length;
  const activeWorlds = worlds.filter((w) => worldHeadcount(w) > 0).length;
  const sceneUsers = Math.max(
    c.scene_users_total,
    scenes.reduce((n, s) => n + s.count, 0),
  );
  const worldUsers = Math.max(
    c.world_users_total,
    c.worlds_live_total ?? 0,
    worlds.reduce((n, w) => n + worldHeadcount(w), 0),
  );
  return {
    peers: Math.max(c.peers_count, sceneUsers + worldUsers),
    scenes: c.scenes_polled || scenes.length,
    worlds: c.worlds_polled || worlds.length,
    sceneUsers,
    worldUsers,
    activeScenes: activeScenes || c.hot_scenes_count,
    activeWorlds: activeWorlds || c.active_worlds,
  };
}
