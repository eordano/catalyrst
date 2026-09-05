import { catalystBase, getJSON } from "../client";
import type { GetOptions } from "../client";
import { placesApiPath } from "../typed";
import {
  currentScenesPath,
  currentWorldsPath,
  fetchCurrentSceneRows,
  fetchCurrentWorldRows,
  worldHistoryPath,
  sceneHistoryPath,
} from "../places/presence-history";
import {
  loadWorldOccupancyHistory,
  loadSceneOccupancyHistory,
} from "../places/presence-history.server";
import {
  loadLiveData,
  loadMyWorlds,
  loadPlatformStatus,
  loadWalletStats,
} from "../wcs.server";
import { RealmAboutSchema, WorldAboutSchema, parsePlacesWorlds } from "./activity";
import { parseNames } from "./manage-worlds";
import {
  SOURCE_REGISTRY,
  isProbeable,
  type SourceEntry,
} from "./data-sources";
import {
  DEFAULT_CADENCE_SECONDS,
  endpointLabel,
  liveNow,
  notProbed,
  sampledAt,
  unavailableFrom,
  type Datum,
} from "./datum.server";

export type ProbeContext = {
  /** the address the screen is scoped to, if any */
  address?: string | null;
  /** a world to probe per-world endpoints with, if any */
  world?: string | null;
  /** a pointer to probe the scene history with, if any */
  pointer?: string | null;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  wcsBase?: string;
};

export type Probe = (ctx: ProbeContext) => Promise<Datum<unknown>>;

export type ProbedSource = SourceEntry & { probe?: Probe; result?: Datum<unknown> };

const PROBE_TIMEOUT_MS = 4000;

function get(ctx: ProbeContext): GetOptions {
  return { signal: ctx.signal, fetchImpl: ctx.fetchImpl };
}

function label(path: string): string {
  return endpointLabel("GET", `${catalystBase()}${path}`);
}

/**
 * A probe exists if and only if the row claims `live` or `sampled`.
 *
 * Probing something that does not exist is theatre, and a row that claims
 * "live" without checking is decorative. `data-sources.test.ts` asserts the
 * biconditional over the assembled registry.
 */
const PROBES: Partial<Record<string, Probe>> = {
  "wcs-worlds": (ctx) =>
    ctx.address
      ? loadMyWorlds(ctx.address, {
          base: ctx.wcsBase,
          signal: ctx.signal,
          fetchImpl: ctx.fetchImpl,
        })
      : Promise.resolve(
          notProbed(
            "GET worlds-content-server.decentraland.org/worlds?authorized_deployer=",
            "an address",
          ),
        ),

  "wcs-wallet-stats": (ctx) =>
    ctx.address
      ? loadWalletStats(ctx.address, {
          base: ctx.wcsBase,
          signal: ctx.signal,
          fetchImpl: ctx.fetchImpl,
        })
      : Promise.resolve(
          notProbed(
            "GET worlds-content-server.decentraland.org/wallet/{address}/stats",
            "an address",
          ),
        ),

  "wcs-live-data": (ctx) =>
    loadLiveData({
      base: ctx.wcsBase,
      signal: ctx.signal,
      fetchImpl: ctx.fetchImpl,
    }),

  "wcs-status": (ctx) =>
    loadPlatformStatus({
      base: ctx.wcsBase,
      signal: ctx.signal,
      fetchImpl: ctx.fetchImpl,
    }),

  "places-worlds": async (ctx) => {
    const path = placesApiPath("get", "/api/worlds");
    if (!ctx.world) return notProbed(label(path), "a world name");
    const endpoint = `${label(path)}?names=${ctx.world}`;
    try {
      const raw = await getJSON<unknown>(path, {
        ...get(ctx),
        query: { names: ctx.world },
      });
      const rows = parsePlacesWorlds(raw);
      if (rows === null) {
        return unavailableFrom(new Error("unexpected payload shape"), endpoint);
      }
      return liveNow(rows, endpoint);
    } catch (err) {
      return unavailableFrom(err, endpoint);
    }
  },

  "world-about": async (ctx) => {
    if (!ctx.world) return notProbed(label("/world/{world}/about"), "a world name");
    const path = `/world/${encodeURIComponent(ctx.world)}/about`;
    const endpoint = label(path);
    try {
      const raw = await getJSON<unknown>(path, get(ctx));
      const parsed = WorldAboutSchema.safeParse(raw);
      if (!parsed.success) {
        return unavailableFrom(new Error("unexpected payload shape"), endpoint);
      }
      return liveNow(parsed.data, endpoint);
    } catch (err) {
      return unavailableFrom(err, endpoint);
    }
  },

  "world-permissions": async (ctx) => {
    if (!ctx.world) {
      return notProbed(label("/world/{world}/permissions"), "a world name");
    }
    const path = `/world/${encodeURIComponent(ctx.world)}/permissions`;
    const endpoint = label(path);
    try {
      const raw = await getJSON<unknown>(path, get(ctx));
      return liveNow(raw, endpoint);
    } catch (err) {
      return unavailableFrom(err, endpoint);
    }
  },

  "realm-about": async (ctx) => {
    const endpoint = label("/about");
    try {
      const raw = await getJSON<unknown>("/about", get(ctx));
      const parsed = RealmAboutSchema.safeParse(raw);
      if (!parsed.success) {
        return unavailableFrom(new Error("unexpected payload shape"), endpoint);
      }
      return liveNow(parsed.data, endpoint);
    } catch (err) {
      return unavailableFrom(err, endpoint);
    }
  },

  "lambdas-names": async (ctx) => {
    if (!ctx.address) {
      return notProbed(label("/lambdas/users/{address}/names"), "an address");
    }
    const path = `/lambdas/users/${encodeURIComponent(
      ctx.address.trim().toLowerCase(),
    )}/names`;
    const endpoint = label(path);
    try {
      const raw = await getJSON<{ elements?: unknown[] }>(path, {
        ...get(ctx),
        query: { pageSize: 100 },
      });
      return liveNow(parseNames(raw?.elements ?? []), endpoint);
    } catch (err) {
      return unavailableFrom(err, endpoint);
    }
  },

  "presence-current-worlds": async (ctx) => {
    const endpoint = label(currentWorldsPath());
    try {
      const rows = await fetchCurrentWorldRows(get(ctx));
      const takenAt = rows[0]?.taken_at;
      if (!takenAt) {
        return notProbed(endpoint, "a snapshot with at least one live world");
      }
      return sampledAt(rows, endpoint, takenAt, DEFAULT_CADENCE_SECONDS);
    } catch (err) {
      return unavailableFrom(err, endpoint);
    }
  },

  "presence-current-scenes": async (ctx) => {
    const endpoint = label(currentScenesPath());
    try {
      const rows = await fetchCurrentSceneRows(get(ctx));
      const takenAt = rows[0]?.taken_at;
      if (!takenAt) {
        return notProbed(endpoint, "a snapshot with at least one occupied scene");
      }
      return sampledAt(rows, endpoint, takenAt, DEFAULT_CADENCE_SECONDS);
    } catch (err) {
      return unavailableFrom(err, endpoint);
    }
  },

  "presence-world-history": (ctx) =>
    ctx.world
      ? loadWorldOccupancyHistory(ctx.world, 1, get(ctx))
      : Promise.resolve(notProbed(label(worldHistoryPath()), "a world name")),

  "presence-scene-history": (ctx) =>
    ctx.pointer
      ? loadSceneOccupancyHistory(ctx.pointer, 1, get(ctx))
      : Promise.resolve(notProbed(label(sceneHistoryPath()), "a parcel pointer")),
};

/** The registry with probes attached -- the invariant lives here, not in a comment. */
export function sourceRegistry(): ProbedSource[] {
  return SOURCE_REGISTRY.map((entry) => {
    const probe = PROBES[entry.id];
    return probe ? { ...entry, probe } : { ...entry };
  });
}

/** A probe never gets to hold the ledger open: the caller's signal still
 *  aborts it, and so does a 4s ceiling, whichever comes first. */
function withTimeout(ctx: ProbeContext): ProbeContext {
  const timeout = AbortSignal.timeout(PROBE_TIMEOUT_MS);
  if (!ctx.signal) return { ...ctx, signal: timeout };
  const any = (AbortSignal as { any?: (s: AbortSignal[]) => AbortSignal }).any;
  return {
    ...ctx,
    signal: any ? any([ctx.signal, timeout]) : ctx.signal,
  };
}

/**
 * Runs every probe, in parallel, with a 4s ceiling. `unbuilt` and `excluded`
 * rows are constants and are never probed. A probe that throws becomes an
 * `unavailable` result like any other failed read.
 */
export async function probeSources(
  ctx: ProbeContext = {},
): Promise<ProbedSource[]> {
  const entries = sourceRegistry();
  const bounded = withTimeout(ctx);
  const results = await Promise.allSettled(
    entries.map((e) => (e.probe ? e.probe(bounded) : Promise.resolve(undefined))),
  );
  return entries.map((entry, i) => {
    if (!entry.probe) return entry;
    const r = results[i];
    return {
      ...entry,
      result:
        r.status === "fulfilled"
          ? (r.value as Datum<unknown>)
          : unavailableFrom(r.reason, entry.endpoint),
    };
  });
}

export { isProbeable, SOURCE_REGISTRY };
