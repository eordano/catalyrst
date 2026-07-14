import { z } from "zod";

import type { ManagedWorld } from "./manage-worlds";
import type { BucketizedHistory } from "../places/presence-history";

/*
 * Pure half of the Activity screens: view-model types, the wire schemas that
 * are not owned elsewhere, and the copy derivations.
 *
 * The row joiner itself lives in `activity.server.ts`, not here, because it
 * builds `Datum`s and the only sanctioned constructors sit behind
 * `datum.server.ts`. A `.ts` module must not import a `.server.ts` module or
 * the server-only boundary stops meaning anything.
 */

const strOrNull = z
  .string()
  .nullish()
  .transform((v) => v ?? null);

/** `GET {catalyst}/world/{name}/about` -- 404s when the world has no scenes. */
export const WorldAboutSchema = z.object({
  healthy: z
    .boolean()
    .nullish()
    .transform((v) => v ?? null),
  acceptingUsers: z
    .boolean()
    .nullish()
    .transform((v) => v ?? null),
  spawnCoordinates: strOrNull,
  configurations: z
    .object({
      scenesUrn: z.array(z.string()),
      realmName: strOrNull,
    })
    .nullish()
    .transform((v) => v ?? null),
});
export type WorldAbout = z.infer<typeof WorldAboutSchema>;

/** `GET {catalyst}/about` -- realm context for the index screen. */
export const RealmAboutSchema = z.object({
  healthy: z
    .boolean()
    .nullish()
    .transform((v) => v ?? null),
  acceptingUsers: z
    .boolean()
    .nullish()
    .transform((v) => v ?? null),
  content: z
    .object({ synchronizationStatus: strOrNull })
    .nullish()
    .transform((v) => v ?? null),
  configurations: z
    .object({ realmName: strOrNull })
    .nullish()
    .transform((v) => v ?? null),
});
export type RealmAbout = z.infer<typeof RealmAboutSchema>;

/**
 * One row of `GET {catalyst}/places/api/worlds?names={world}`.
 *
 * `user_visits` and `user_count` are parsed but deliberately NOT part of the
 * reception view model: both read 0 for every world sampled, and a wrong number
 * is worse than an absent one. They appear once, in the EXCLUDED group of the
 * data-sources ledger, so nobody rediscovers them and wires them up.
 */
export const PlacesWorldRowSchema = z.object({
  id: z.string(),
  world_name: strOrNull,
  title: strOrNull,
  likes: z
    .number()
    .nullish()
    .transform((v) => v ?? null),
  dislikes: z
    .number()
    .nullish()
    .transform((v) => v ?? null),
  favorites: z
    .number()
    .nullish()
    .transform((v) => v ?? null),
  like_rate: z
    .number()
    .nullish()
    .transform((v) => v ?? null),
  deployed_at: strOrNull,
});
export type PlacesWorldRow = z.infer<typeof PlacesWorldRowSchema>;

export const PlacesWorldsEnvelopeSchema = z.object({
  data: z.array(z.unknown()),
  total: z.number(),
});

/**
 * `null` means the envelope did not parse -- which is NOT the same as Places
 * having no record of the world. Callers must render an unavailable state for
 * it; an empty array here is a real "Places knows nothing about this world".
 */
export function parsePlacesWorlds(raw: unknown): PlacesWorldRow[] | null {
  const env = PlacesWorldsEnvelopeSchema.safeParse(raw);
  if (!env.success) return null;
  const out: PlacesWorldRow[] = [];
  for (const r of env.data.data) {
    const parsed = PlacesWorldRowSchema.safeParse(r);
    if (parsed.success) out.push(parsed.data);
  }
  return out;
}

export const PLACES_EXCLUDED_FIELDS_SENTENCE =
  "That response also carries `user_visits` and `user_count`. Both read 0 for every world we sampled, so they are not shown.";

function hhmmss(iso: string): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "an unknown time";
  return `${new Date(t).toISOString().slice(11, 19)} UTC`;
}

/** The mandated note for a world that was not in the last presence snapshot. */
export function noSampleNote(takenAt: string): string {
  return `No sample: this world was not live at the last snapshot (${hhmmss(
    takenAt,
  )}). Not the same as zero.`;
}

/** The mandated note beside a literal 0 that came back from a real read. */
export function realZeroNote(takenAt: string): string {
  return `a real zero \u{2014} sampled at ${hhmmss(takenAt)}, nobody in`;
}

/**
 * `/live-data` lists only rooms that currently have users, so a world missing
 * from `perWorld` on a successful read is a genuine zero. The note says how
 * that was derived rather than leaving the reader to assume a measurement.
 */
export const LIVE_DATA_ZERO_NOTE =
  "a real zero \u{2014} /live-data lists only rooms with users in them, and this world was not in that set at read time";

export const NEVER_DEPLOYED_REASON =
  "No scene deployed to this NAME, so presence has never had anything to sample.";
export const NEVER_DEPLOYED_TODAY =
  "Publish a scene to this NAME from /creator-hub/deploy-world.";

/**
 * Presence and `/live-data` measure different things and routinely disagree.
 * Never reconcile them, never `Math.max` them -- say so instead. Returns null
 * when they agree or when either side is missing.
 */
export function disagreementSentence(
  presenceCount: number | null,
  liveUsers: number | null,
): string | null {
  if (presenceCount === null || liveUsers === null) return null;
  if (presenceCount === liveUsers) return null;
  return `These disagree (${presenceCount} vs ${liveUsers}) and both are right: presence samples every 5 minutes and counts distinct addresses in comms; /live-data is instant and is the worlds server's own figure. Neither is "users online".`;
}

/** Why a world row cannot show a headcount, when it cannot. */
export type WorldRowKind = "deployed" | "never-deployed" | "blocked";

export function worldRowKind(world: ManagedWorld): WorldRowKind {
  if (world.blockedSince) return "blocked";
  return world.deployedScenes > 0 ? "deployed" : "never-deployed";
}

export type SceneUrn = { urn: string; baseUrl: string | null };

export function parseSceneUrn(raw: string): SceneUrn {
  const [urn, qs] = raw.split("?");
  if (!qs) return { urn: raw, baseUrl: null };
  const params = new URLSearchParams(qs.replace(/^[?&]*/, ""));
  return { urn: urn ?? raw, baseUrl: params.get("baseUrl") };
}

export type WorldHistoryView = BucketizedHistory;

export const HISTORY_LIMIT_WORLD_PAGE = 5000;
/** 7 days of 5-minute snapshots. */
export const HISTORY_LIMIT_PEAK_7D = 2016;
