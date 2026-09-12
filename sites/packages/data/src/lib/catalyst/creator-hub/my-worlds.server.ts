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

export type WorldOrigin = "catalyst.example.com" | "upstream" | "both";

export type UnionedWorld = ManagedWorld & { origin: WorldOrigin };

export type MyWorldsUnion = {
  address: string;
  rows: UnionedWorld[];
  dclOne: Datum<DclName[]>;
  upstream: Datum<ManagedWorld[]>;
  partial: boolean;
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
    deployedScenes: 0,
    thumbnail: null,
    role: "owner",
  };
}

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
