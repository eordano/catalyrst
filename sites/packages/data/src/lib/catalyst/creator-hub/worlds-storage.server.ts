import {
  fetchEnvKeys,
  fetchPlayers,
  fetchUsage,
  fetchValues,
  normalizeAddress,
  type EnvKey,
  type PlayerEntry,
  type StorageValue,
  type StorageWorld,
  type WalletStats,
  type WorldsStorageData,
} from "./worlds-storage";
import { loadManageWorlds } from "./manage-worlds.server";
import type { GetOptions } from "../client";

/**
 * The real header tile: deployed bytes and quota from worlds-content-server.
 *
 * The `/world-storage/*` reads below stay exactly as they are. They resolve to
 * a 400 Invalid Auth Chain (ADR-44 -- that endpoint needs a signed fetch made by
 * the scene runtime) and they are the honest record of a gated endpoint; they
 * become live for free the day the edge routes them. They also measure a
 * different thing: the KV store a scene writes at runtime, not deployed bytes.
 */
export { loadWalletStats } from "../wcs.server";

const EMPTY_PLAYERS: PlayerEntry = { addresses: [], profileNames: {} };

export async function loadWorldsStorage(
  address: string | null | undefined,
  opts: GetOptions = {},
): Promise<WorldsStorageData> {
  const addr = normalizeAddress(address);

  let source: "live" | "empty" = "empty";
  let fallback = false;

  let values: StorageValue[] = [];
  try {
    values = await fetchValues(opts);
    if (values.length > 0) source = "live";
  } catch {
    fallback = true;
  }

  let envKeys: EnvKey[] = [];
  try {
    envKeys = await fetchEnvKeys(opts);
    if (envKeys.length > 0) source = "live";
  } catch {
    fallback = true;
  }

  let players: PlayerEntry = EMPTY_PLAYERS;
  try {
    const live = await fetchPlayers(opts);
    if (live.length > 0) {
      players = { addresses: live, profileNames: {} };
      source = "live";
    }
  } catch {
    fallback = true;
  }

  let worlds: StorageWorld[] = [];
  if (addr) {
    try {
      const managed = await loadManageWorlds(addr, opts.signal);
      worlds = managed.worlds.map((w) => ({
        name: w.name,
        role: w.role === "owner" ? ("owner" as const) : ("collaborator" as const),
        scenes: w.deployedScenes,
        usedBytes: 0,
        maxTotalSizeBytes: 0,
      }));
      if (worlds.length > 0) source = "live";
    } catch {
      fallback = true;
    }
  }

  let stats: WalletStats | null = null;
  try {
    const usage = await fetchUsage("world", opts);
    if (usage.maxTotalSizeBytes > 0) {
      stats = {
        wallet: addr,
        usedSpace: usage.usedBytes,
        maxAllowedSpace: usage.maxTotalSizeBytes,
        dclNames: [],
        ensNames: [],
      };
      source = "live";
    }
  } catch {
    fallback = true;
  }

  return {
    address: addr,
    stats,
    worlds,
    lands: [],
    scope: null,
    values,
    envKeys,
    players,
    source,
    fallback,
  };
}
