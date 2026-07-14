import { z } from "zod";

import { CatalystError, getJSON } from "../client";
import type { GetOptions } from "../client";
import {
  parseManagedWorlds,
  parseNames,
  normalizeAddress,
  type ManagedWorld,
  type DclName,
} from "./manage-worlds";

export type ManageWorldsData = {
  address: string;
  worlds: ManagedWorld[];
  names: DclName[];
};

const WorldAboutSchema = z.object({
  configurations: z
    .object({
      scenesUrn: z.array(z.string()),
    })
    .nullish()
    .transform((v) => v ?? null),
});

export function worldNameForName(name: string): string {
  const bare = name
    .trim()
    .toLowerCase()
    .replace(/\.(dcl\.eth|eth)$/i, "");
  return `${bare}.dcl.eth`;
}

async function fetchLiveNames(
  address: string,
  opts: GetOptions = {},
): Promise<DclName[]> {
  const raw = await getJSON<{ elements?: unknown[] }>(
    `/lambdas/users/${encodeURIComponent(normalizeAddress(address))}/names`,
    { ...opts, query: { pageSize: 100 } },
  );
  return parseNames(raw?.elements ?? []);
}

/**
 * A 404 from `/world/{name}/about` is catalyst's answer for a NAME with no
 * scenes deployed, so it is a real zero. Anything else -- a broken read, a
 * payload we cannot parse, a response with no configurations block -- is not a
 * count, and throwing keeps it out of the world list rather than publishing it
 * as "0 scenes, unpublished".
 */
async function resolveWorldScenes(
  worldName: string,
  opts: GetOptions = {},
): Promise<number> {
  const path = `/world/${encodeURIComponent(worldName)}/about`;
  let about: unknown;
  try {
    about = await getJSON<unknown>(path, opts);
  } catch (err) {
    if (err instanceof CatalystError && err.status === 404) return 0;
    throw err;
  }
  const parsed = WorldAboutSchema.safeParse(about);
  if (!parsed.success || parsed.data.configurations === null) {
    throw new CatalystError("world about carried no scene list", path);
  }
  return parsed.data.configurations.scenesUrn.length;
}

async function fetchLiveWorlds(
  names: DclName[],
  address: string,
  opts: GetOptions = {},
): Promise<ManagedWorld[]> {
  const owner = normalizeAddress(address);
  const resolved = await Promise.all(
    names.map(async (n) => {
      const worldName = worldNameForName(n.name);
      const deployedScenes = await resolveWorldScenes(worldName, opts);
      return {
        name: worldName,
        owner,
        title: null,
        deployedScenes,
        role: "owner" as const,
      };
    }),
  );
  return parseManagedWorlds(resolved);
}

export async function loadManageWorlds(
  address: string,
  signal?: AbortSignal,
): Promise<ManageWorldsData> {
  const wallet = normalizeAddress(address);
  if (!wallet) {
    return {
      address,
      worlds: [],
      names: [],
    };
  }

  const names = await fetchLiveNames(address, { signal });
  const worlds = await fetchLiveWorlds(names, address, { signal });

  return {
    address,
    worlds,
    names,
  };
}
