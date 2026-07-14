import type { AuthIdentity } from "../../auth/types";
import { signedGetJSON } from "../client";
import type { CreatorScenesStats } from "@ui/creatorhub/lib/scene-analytics";
import { parseCreatorScenesStats } from "./scene-analytics.gen";


export {
  parseCreatorScenesStats,
  WireCreatorScenesStatsSchema,
} from "./scene-analytics.gen";

export const SCENE_STATS_PATH = "/creators/me/scenes/stats";

const DEFAULT_CREATORS_DATA_BASE = "https://decentraland.org/creators-data/api";

export function creatorsDataBase(override?: string): string {
  const base =
    override ??
    (typeof process !== "undefined"
      ? process.env?.CREATORS_DATA_URL
      : undefined) ??
    DEFAULT_CREATORS_DATA_BASE;
  return base.replace(/\/$/, "");
}

export type FetchSceneStatsOptions = {
  base?: string;
  signal?: AbortSignal;
};

export async function fetchCreatorScenesStats(
  identity: AuthIdentity,
  opts: FetchSceneStatsOptions = {},
): Promise<CreatorScenesStats> {
  const raw = await signedGetJSON<unknown>(SCENE_STATS_PATH, {
    identity,
    base: creatorsDataBase(opts.base),
    signal: opts.signal,
  });
  return parseCreatorScenesStats(raw);
}
