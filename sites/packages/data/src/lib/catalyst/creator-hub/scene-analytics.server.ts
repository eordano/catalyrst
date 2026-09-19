import { z } from "zod";
import { getJSON, type GetOptions } from "../client";
import { fetchSceneHistory, fetchWorldHistory } from "../places/presence-history";

const Place = z.object({
  title: z.string().nullish(),
  base_position: z.string().nullish(),
  world: z.boolean(),
  world_name: z.string().nullish(),
  creator_address: z.string(),
});
const Places = z.object({ data: z.array(Place), total: z.number().int().nonnegative() });
type Sample = { taken_at: string; count: number };

export function sampledPeaks(samples: Sample[], asOf: string) {
  const daily = new Map<string, number>();
  const end = new Date(`${asOf}T00:00:00Z`).getTime();
  const start = end - 90 * 86400000;
  const simultaneous = new Map<string, number>();
  for (const sample of samples) {
    const time = Date.parse(sample.taken_at);
    if (!Number.isFinite(time) || time < start || time >= end) continue;
    simultaneous.set(sample.taken_at, (simultaneous.get(sample.taken_at) ?? 0) + sample.count);
  }
  for (const [timestamp, count] of simultaneous) {
    const day = new Date(timestamp).toISOString().slice(0, 10);
    daily.set(day, Math.max(daily.get(day) ?? 0, count));
  }
  const window = (days: number) => {
    const since = new Date(end - days * 86400000).toISOString().slice(0, 10);
    const values = [...daily].filter(([day]) => day >= since).map(([, count]) => count);
    return { peak_concurrent_users: values.length ? Math.max(...values) : null };
  };
  return {
    windows: { yesterday: window(1), last_7d: window(7), last_30d: window(30) },
    daily: [...daily].sort(([a], [b]) => a.localeCompare(b)).map(([date, count]) => ({ date, peak_concurrent_users: count })),
  };
}

export async function loadCreatorSceneStats(address: string, opts: GetOptions = {}) {
  const wallet = address.toLowerCase();
  if (!/^0x[0-9a-f]{40}$/.test(wallet)) throw new Error("Invalid creator address");
  const places = new Map<string, z.infer<typeof Place>>();
  for (let offset = 0; ; offset += 100) {
    const page = Places.parse(await getJSON("/places/api/places", {
      ...opts, query: { creator_address: wallet, limit: 100, offset },
    }));
    for (const place of page.data) {
      if (place.creator_address.toLowerCase() !== wallet) throw new Error("Places returned a different creator");
      const key = place.world ? place.world_name : place.base_position;
      if (key) places.set(`${place.world}:${key.toLowerCase()}`, place);
    }
    if (offset + page.data.length >= page.total) break;
    if (page.data.length === 0 || offset >= 9900) throw new Error("Incomplete creator scene listing");
  }
  const asOf = new Date().toISOString().slice(0, 10);
  const scenes = [];
  const list = [...places.values()];
  for (let offset = 0; offset < list.length; offset += 4) {
    scenes.push(...await Promise.all(list.slice(offset, offset + 4).map(async place => {
      const sceneId = (place.world ? place.world_name : place.base_position)!;
      const history = place.world
        ? (await fetchWorldHistory(sceneId, 5000, opts)).map(row => ({ ...row, count: row.live_users ?? row.count }))
        : await fetchSceneHistory(sceneId, 5000, opts);
      return {
        scene_type: place.world ? "world" : "genesis", scene_id: sceneId,
        title: place.title ?? null, deployer_address: wallet,
        ...sampledPeaks(history, asOf), retention: {}, retention_series: [], deploy_dates: [],
      };
    })));
  }
  return { address: wallet, as_of: asOf, scenes };
}
