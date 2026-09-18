import { z } from "zod";

import { CatalystError, getJSON } from "../client";
import type { GetOptions } from "../client";
import { withDeadline } from "../../request-deadline";
import { fetchWorldsRealm, personalWorldName } from "./deploy-world";
import {
  parseManagedWorlds,
  parseNames,
  normalizeAddress,
  type ManagedWorld,
  type DclName,
} from "./manage-worlds";

type ManageWorldsData = {
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

function worldNamesFor(names: DclName[], address: string, personalWorlds: boolean): string[] {
  const out = names.map((n) => worldNameForName(n.name));
  const personal = personalWorlds ? personalWorldName(address) : null;
  if (personal && !out.includes(personal)) out.push(personal);
  return out;
}

async function realmOffersPersonalWorlds(opts: GetOptions): Promise<boolean> {
  try {
    return (await fetchWorldsRealm(opts)).personalWorlds;
  } catch {
    return false;
  }
}

async function fetchLiveWorlds(
  names: DclName[],
  address: string,
  personalWorlds: boolean,
  counts: Map<string, number>,
  opts: GetOptions = {},
): Promise<ManagedWorld[]> {
  const owner = normalizeAddress(address);
  const resolved = await Promise.all(
    worldNamesFor(names, address, personalWorlds).map(async (worldName) => {
      const deployedScenes = counts.get(worldName) ?? await resolveWorldScenes(worldName, opts);
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

async function fetchWorldCounts(address: string, opts: GetOptions): Promise<Map<string, number>> {
  try {
    const raw = await withDeadline((signal) => getJSON<unknown>("/worlds", {
      ...opts,
      signal,
      query: { authorized_deployer: address, limit: 1000 },
    }), 1000, opts.signal);
    const parsed = z.object({ worlds: z.array(z.object({
      name: z.string(),
      deployed_scenes: z.number().int().nonnegative(),
    })) }).parse(raw);
    return new Map(parsed.worlds.map((world) => [world.name.toLowerCase(), world.deployed_scenes]));
  } catch {
    opts.signal?.throwIfAborted();
    return new Map();
  }
}

export async function loadManageWorlds(
  address: string,
  signal?: AbortSignal,
  opts: Pick<GetOptions, "fetchImpl"> = {},
): Promise<ManageWorldsData> {
  const wallet = normalizeAddress(address);
  if (!wallet) {
    return {
      address,
      worlds: [],
      names: [],
    };
  }

  const get: GetOptions = { signal, fetchImpl: opts.fetchImpl };
  const [names, personalWorlds, counts] = await Promise.all([
    fetchLiveNames(address, get),
    realmOffersPersonalWorlds(get),
    fetchWorldCounts(wallet, get),
  ]);
  const worlds = await fetchLiveWorlds(names, address, personalWorlds, counts, get);

  return {
    address,
    worlds,
    names,
  };
}
