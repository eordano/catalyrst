import { getJSON, buildQuery } from "./client";
import type { GetOptions, Query } from "./client";
import {
  LiveDataSchema,
  PlatformStatusSchema,
  WCS_LIVE_DATA_PATH,
  WCS_STATUS_PATH,
  WCS_WORLDS_PATH,
  WalletStatsSchema,
  parseWcsWorlds,
  parseWcsWorldsTotal,
  wcsBase,
  wcsWalletStatsPath,
  type LiveData,
  type ManagedWorld,
  type PlatformStatus,
  type WalletStats,
} from "./wcs";
import {
  endpointLabel,
  liveNow,
  unavailableFrom,
  type Datum,
} from "./creator-hub/datum.server";

export type WcsOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
};

const WORLDS_PAGE_LIMIT = 100;

function get(opts: WcsOptions, base: string): GetOptions {
  return { base, signal: opts.signal, fetchImpl: opts.fetchImpl };
}

function label(base: string, path: string, query?: Query): string {
  return endpointLabel("GET", `${base}${path}${buildQuery(query)}`);
}

export type MyWorlds = { worlds: ManagedWorld[]; total: number };

/**
 * `GET {wcs}/worlds?authorized_deployer={address}&limit=100&sort=last_deployed_at&order=desc`
 *
 * A 200 with no rows is a real answer and returns `{state:"live", value: []}`.
 * A failed read returns `unavailable` -- never `[]`, because an empty list and a
 * dead host must not render the same.
 */
export async function loadMyWorlds(
  address: string,
  opts: WcsOptions = {},
): Promise<Datum<MyWorlds>> {
  const base = wcsBase(opts.base);
  const query = {
    authorized_deployer: address.trim().toLowerCase(),
    limit: WORLDS_PAGE_LIMIT,
    sort: "last_deployed_at",
    order: "desc",
  };
  const endpoint = label(base, WCS_WORLDS_PATH, query);
  try {
    const raw = await getJSON<unknown>(WCS_WORLDS_PATH, {
      ...get(opts, base),
      query,
    });
    return liveNow(
      { worlds: parseWcsWorlds(raw, base), total: parseWcsWorldsTotal(raw) },
      endpoint,
    );
  } catch (err) {
    return unavailableFrom(
      err,
      endpoint,
      "The world list is the whole table on this screen, so nothing below is shown.",
    );
  }
}

/**
 * `GET {wcs}/wallet/{address}/stats` -- deployed-content bytes and quota.
 * `usedSpace` / `maxAllowedSpace` / `dclNames[].size` are decimal byte strings;
 * callers parse them with `bytesFromString` (BigInt), never `Number`.
 */
export async function loadWalletStats(
  address: string,
  opts: WcsOptions = {},
): Promise<Datum<WalletStats>> {
  const base = wcsBase(opts.base);
  const path = wcsWalletStatsPath(address);
  const endpoint = label(base, path);
  try {
    const raw = await getJSON<unknown>(path, get(opts, base));
    const parsed = WalletStatsSchema.safeParse(raw);
    if (!parsed.success) {
      return unavailableFrom(
        new Error("unexpected payload shape"),
        endpoint,
        "The response did not match the wallet-stats shape, so no size is shown.",
      );
    }
    return liveNow(parsed.data, endpoint);
  } catch (err) {
    return unavailableFrom(err, endpoint);
  }
}

/**
 * `GET {wcs}/live-data` -- the worlds server's own instantaneous comms figure.
 * Real-time read: staleness is undefined for it, so it is never `sampled`.
 */
export async function loadLiveData(opts: WcsOptions = {}): Promise<Datum<LiveData>> {
  const base = wcsBase(opts.base);
  const endpoint = label(base, WCS_LIVE_DATA_PATH);
  try {
    const raw = await getJSON<unknown>(WCS_LIVE_DATA_PATH, get(opts, base));
    const parsed = LiveDataSchema.safeParse(raw);
    if (!parsed.success) {
      return unavailableFrom(
        new Error("unexpected payload shape"),
        endpoint,
        "The response did not match the live-data shape.",
      );
    }
    return liveNow(parsed.data, endpoint);
  } catch (err) {
    return unavailableFrom(err, endpoint);
  }
}

/** `GET {wcs}/status` -- platform totals (world counts, comms rooms/users). */
export async function loadPlatformStatus(
  opts: WcsOptions = {},
): Promise<Datum<PlatformStatus>> {
  const base = wcsBase(opts.base);
  const endpoint = label(base, WCS_STATUS_PATH);
  try {
    const raw = await getJSON<unknown>(WCS_STATUS_PATH, get(opts, base));
    const parsed = PlatformStatusSchema.safeParse(raw);
    if (!parsed.success) {
      return unavailableFrom(
        new Error("unexpected payload shape"),
        endpoint,
        "The response did not match the status shape.",
      );
    }
    return liveNow(parsed.data, endpoint);
  } catch (err) {
    return unavailableFrom(err, endpoint);
  }
}

export { wcsBase } from "./wcs";
