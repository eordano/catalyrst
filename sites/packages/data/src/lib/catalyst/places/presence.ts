import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import {
  CurrentSnapshotSchema,
  SceneOccupancyRowSchema,
  WorldHeadcountRowSchema,
} from "../generated-schemas/presence";

export const PRESENCE_BASE = "/presence";

export { CurrentSnapshotSchema, SceneOccupancyRowSchema, WorldHeadcountRowSchema };
export type CurrentSnapshot = z.infer<typeof CurrentSnapshotSchema>;
export type SceneOccupancyRow = z.infer<typeof SceneOccupancyRowSchema>;
export type WorldHeadcountRow = z.infer<typeof WorldHeadcountRowSchema>;

export type PresenceSnapshot = {
  current: CurrentSnapshot | null;
  scenes: SceneOccupancyRow[] | null;
  worlds: WorldHeadcountRow[] | null;
  source: "catalyst" | "unavailable";
};

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
