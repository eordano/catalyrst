import { z } from "zod";

import { getJSON } from "../client";
import { fetchSalesByContracts } from "../marketplace/activity";
import { fetchCollections, toCollectionCard } from "../builder/collections";
import { fetchCreations } from "../marketplace/index";
import { sceneJumpUrl, worldJumpUrl } from "../places/presence";
import {
  countPublished,
  rollupSales,
  DAY_MS,
  type SceneVisitRow,
  type ScenesSummary,
  type Summary,
} from "./metrics";

const WINDOW_DAYS = 7;

export type CreatorMetricsData = {
  address: string;
  windowDays: number;
  summary: Summary;
  sceneRows: SceneVisitRow[];
  empty: boolean;
  loadError: boolean;
};

const PlacesRowSchema = z.object({
  title: z
    .string()
    .nullish()
    .transform((v) => v ?? null),
  base_position: z
    .string()
    .nullish()
    .transform((v) => v ?? null),
  world: z.boolean(),
  world_name: z.string().nullish(),
  user_visits: z.number().nullish(),
  user_count: z.number().nullish(),
});
const PlacesEnvelopeSchema = z.object({
  data: z.array(z.unknown()),
  total: z.number(),
});

async function fetchCreatorScenes(
  address: string,
  signal?: AbortSignal,
): Promise<{ summary: ScenesSummary; rows: SceneVisitRow[] }> {
  const raw = await getJSON<unknown>("/places/api/places", {
    signal,
    query: {
      creator_address: address.toLowerCase(),
      limit: 100,
      with_realtime_active_users: true,
    },
  });
  const env = PlacesEnvelopeSchema.parse(raw);
  let parsed = 0;
  let visits30d = 0;
  let liveNow = 0;
  const seen = new Set<string>();
  const rows: SceneVisitRow[] = [];
  for (const r of env.data) {
    const row = PlacesRowSchema.safeParse(r);
    if (!row.success) continue;
    parsed += 1;
    const world = row.data.world && !!row.data.world_name;
    const location = world
      ? (row.data.world_name as string)
      : row.data.base_position;
    if (location) {
      if (seen.has(location)) continue;
      seen.add(location);
    }
    visits30d += row.data.user_visits ?? 0;
    liveNow += row.data.user_count ?? 0;
    rows.push({
      title: row.data.title || location || "Untitled scene",
      location: location ?? null,
      href: world
        ? worldJumpUrl(row.data.world_name as string)
        : row.data.base_position
          ? sceneJumpUrl(row.data.base_position)
          : null,
      visits30d: row.data.user_visits ?? null,
      liveNow: row.data.user_count ?? null,
    });
  }
  rows.sort((a, b) => (b.visits30d ?? -1) - (a.visits30d ?? -1));
  return {
    summary: {
      places: env.total > parsed ? env.total : rows.length,
      visits30d,
      liveNow,
    },
    rows,
  };
}

export async function loadCreatorMetrics(
  address: string,
  signal?: AbortSignal,
): Promise<CreatorMetricsData> {
  const [collections, creations, scenesRes] = await Promise.all([
    fetchCollections(address, { signal }).catch(() => null),
    fetchCreations(address, { first: 1000 }, { signal }).catch(() => null),
    fetchCreatorScenes(address, signal).catch(() => null),
  ]);
  const scenes = scenesRes?.summary ?? null;
  const sceneRows = scenesRes?.rows ?? [];

  const publishedCollections = collections
    ? countPublished(collections.map(toCollectionCard))
    : null;

  const itemsTotal = creations
    ? creations.wearables.length + creations.emotes.length
    : null;
  const onSaleItems = creations
    ? [...creations.wearables, ...creations.emotes].filter(
        (c) => c.price !== null,
      ).length
    : null;

  const contractAddresses = (collections ?? [])
    .map((c) => c.contract_address)
    .filter((c): c is string => !!c);
  const now = Date.now();

  let sales7d: number | null = null;
  let salesVolumeMana7d: number | null = null;
  let salesUnavailable = false;
  if (!collections) {
    salesUnavailable = true;
  } else if (contractAddresses.length === 0) {
    sales7d = 0;
    salesVolumeMana7d = 0;
  } else {
    try {
      const sales = await fetchSalesByContracts(
        { contractAddresses, from: now - WINDOW_DAYS * DAY_MS },
        { signal },
      );
      const rollup = rollupSales(
        sales.data.map((s) => ({ timestamp: s.timestamp, price: s.price })),
        now,
        WINDOW_DAYS,
      );
      sales7d = sales.total;
      salesVolumeMana7d = rollup.volumeMana;
    } catch {
      salesUnavailable = true;
    }
  }

  const empty =
    collections !== null &&
    creations !== null &&
    scenes !== null &&
    collections.length === 0 &&
    itemsTotal === 0 &&
    scenes.places === 0;

  return {
    address,
    windowDays: WINDOW_DAYS,
    summary: {
      publishedCollections,
      onSaleItems,
      sales7d,
      salesVolumeMana7d,
      salesUnavailable,
      scenes,
    },
    sceneRows,
    empty,
    loadError: !collections && !creations && !scenes,
  };
}
