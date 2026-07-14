import { buildQuery, catalystBase, getJSON } from "../client";
import type { GetOptions } from "../client";
import { loadMyWorlds } from "../wcs.server";
import {
  normalizeAddress,
  parseNames,
  type DclName,
  type ManagedWorld,
} from "./manage-worlds";
import { worldNameForName } from "./manage-worlds.server";
import {
  endpointLabel,
  liveNow,
  showable,
  unavailableFrom,
  type Datum,
} from "./datum.server";

/**
 * Two hosts answer "which worlds are mine", and they do not agree:
 *
 *   catalyst `/lambdas/users/{address}/names` -- the NAMEs this address owns on
 *     this stack. A NAME can exist with nothing deployed to it.
 *   worlds-content-server `/worlds?authorized_deployer={address}` -- worlds
 *     upstream has content for, including ones deployed by a collaborator.
 *
 * This is the split stack, and the union states it rather than smoothing it:
 * every row carries where it came from, and a host that failed is named instead
 * of silently shrinking the list.
 */
export type WorldOrigin = "catalyst.example.com" | "upstream" | "both";

export type UnionedWorld = ManagedWorld & { origin: WorldOrigin };

export type MyWorldsUnion = {
  address: string;
  rows: UnionedWorld[];
  dclOne: Datum<DclName[]>;
  upstream: Datum<ManagedWorld[]>;
  /** exactly one host answered -- the list is real but incomplete */
  partial: boolean;
  /** neither host answered -- render an error state, not an empty table */
  bothFailed: boolean;
};

export type MyWorldsUnionOptions = {
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  wcsBase?: string;
};

const NAMES_PAGE_SIZE = 100;

function namesPath(address: string): string {
  return `/lambdas/users/${encodeURIComponent(address)}/names`;
}

async function loadDclOneNames(
  address: string,
  opts: MyWorldsUnionOptions,
): Promise<Datum<DclName[]>> {
  const path = namesPath(address);
  const query = { pageSize: NAMES_PAGE_SIZE };
  const endpoint = endpointLabel(
    "GET",
    `${catalystBase()}${path}${buildQuery(query)}`,
  );
  const get: GetOptions = {
    signal: opts.signal,
    fetchImpl: opts.fetchImpl,
    query,
  };
  try {
    const raw = await getJSON<{ elements?: unknown[] }>(path, get);
    return liveNow(parseNames(raw?.elements ?? []), endpoint);
  } catch (err) {
    return unavailableFrom(
      err,
      endpoint,
      "Worlds that exist only as a NAME on this stack are missing from the list below.",
    );
  }
}

function blankWorld(name: string): ManagedWorld {
  return {
    name,
    owner: null,
    title: null,
    description: null,
    contentRating: null,
    spawnCoordinates: null,
    lastDeployedAt: null,
    blockedSince: null,
    // Not "zero scenes" -- this row came from the NAME registry, which does not
    // know about deployments. The upstream row is what can answer that, and
    // when there is no upstream row the screen must not claim a count.
    deployedScenes: 0,
    thumbnail: null,
    role: "owner",
  };
}

/**
 * Unions the two lists on world name.
 *
 * `deployedScenes` is only ever taken from the upstream row, because only
 * worlds-content-server measures it. A catalyst.example.com-only row keeps 0 *and* is marked
 * `origin: "catalyst.example.com"`, which is the signal the UI needs to avoid printing a
 * scene count that nobody reported.
 */
export function unionWorlds(
  names: DclName[],
  upstream: ManagedWorld[],
): UnionedWorld[] {
  const byName = new Map<string, UnionedWorld>();

  for (const w of upstream) {
    byName.set(w.name.trim().toLowerCase(), { ...w, origin: "upstream" });
  }
  for (const n of names) {
    const world = worldNameForName(n.name);
    const key = world.trim().toLowerCase();
    const existing = byName.get(key);
    if (existing) byName.set(key, { ...existing, origin: "both" });
    else byName.set(key, { ...blankWorld(world), origin: "catalyst.example.com" });
  }

  return [...byName.values()].sort((a, b) => {
    const at = a.lastDeployedAt ? Date.parse(a.lastDeployedAt) || 0 : 0;
    const bt = b.lastDeployedAt ? Date.parse(b.lastDeployedAt) || 0 : 0;
    return bt - at || a.name.localeCompare(b.name);
  });
}

export async function loadMyWorldsUnion(
  address: string,
  opts: MyWorldsUnionOptions = {},
): Promise<MyWorldsUnion> {
  const addr = normalizeAddress(address);

  const settled = await Promise.allSettled([
    loadDclOneNames(addr, opts),
    loadMyWorlds(addr, {
      base: opts.wcsBase,
      signal: opts.signal,
      fetchImpl: opts.fetchImpl,
    }),
  ]);

  const dclOne: Datum<DclName[]> =
    settled[0].status === "fulfilled"
      ? settled[0].value
      : unavailableFrom(settled[0].reason, endpointLabel("GET", `${catalystBase()}${namesPath(addr)}`));

  const upstreamRaw =
    settled[1].status === "fulfilled"
      ? settled[1].value
      : unavailableFrom(
          settled[1].reason,
          "GET worlds-content-server.decentraland.org/worlds?authorized_deployer=",
        );

  const upstream: Datum<ManagedWorld[]> = showable(upstreamRaw)
    ? liveNow(upstreamRaw.value.worlds, upstreamRaw.endpoint)
    : (upstreamRaw as Datum<ManagedWorld[]>);

  const names = showable(dclOne) ? dclOne.value : [];
  const upstreamRows = showable(upstream) ? upstream.value : [];

  const dclOneOk = showable(dclOne);
  const upstreamOk = showable(upstream);

  return {
    address: addr,
    rows: unionWorlds(names, upstreamRows),
    dclOne,
    upstream,
    partial: dclOneOk !== upstreamOk,
    bothFailed: !dclOneOk && !upstreamOk,
  };
}
