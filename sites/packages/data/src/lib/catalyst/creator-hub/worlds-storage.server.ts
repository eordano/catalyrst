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

const EMPTY_PLAYERS: PlayerEntry = { addresses: [], profileNames: {} };

export async function loadWorldsStorage(
  address: string | null | undefined,
  opts: GetOptions = {},
): Promise<WorldsStorageData> {
  const addr = normalizeAddress(address);

  const [valuesRes, envRes, playersRes, managedRes, usageRes] =
    await Promise.allSettled([
      fetchValues(opts),
      fetchEnvKeys(opts),
      fetchPlayers(opts),
      addr
        ? loadManageWorlds(addr, opts.signal, { fetchImpl: opts.fetchImpl })
        : Promise.resolve(null),
      fetchUsage("world", opts),
    ]);

  let source: "live" | "empty" = "empty";
  let fallback = false;

  let values: StorageValue[] = [];
  if (valuesRes.status === "fulfilled") {
    values = valuesRes.value;
    if (values.length > 0) source = "live";
  } else {
    fallback = true;
  }

  let envKeys: EnvKey[] = [];
  if (envRes.status === "fulfilled") {
    envKeys = envRes.value;
    if (envKeys.length > 0) source = "live";
  } else {
    fallback = true;
  }

  let players: PlayerEntry = EMPTY_PLAYERS;
  if (playersRes.status === "fulfilled") {
    if (playersRes.value.length > 0) {
      players = { addresses: playersRes.value, profileNames: {} };
      source = "live";
    }
  } else {
    fallback = true;
  }

  let worlds: StorageWorld[] = [];
  if (managedRes.status === "fulfilled") {
    if (managedRes.value) {
      worlds = managedRes.value.worlds.map((w) => ({
        name: w.name,
        role:
          w.role === "owner" ? ("owner" as const) : ("collaborator" as const),
        scenes: w.deployedScenes,
        usedBytes: 0,
        maxTotalSizeBytes: 0,
      }));
      if (worlds.length > 0) source = "live";
    }
  } else {
    fallback = true;
  }

  let stats: WalletStats | null = null;
  if (usageRes.status === "fulfilled") {
    const usage = usageRes.value;
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
  } else {
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
