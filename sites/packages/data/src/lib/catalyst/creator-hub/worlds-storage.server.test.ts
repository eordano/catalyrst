import { describe, expect, it } from "vitest";

import { loadManageWorlds } from "./manage-worlds.server";
import { loadWorldsStorage } from "./worlds-storage.server";

const ADDR = "0x" + "ab".repeat(20);
const PAGE = { limit: 100, offset: 0, total: 1 };

type Handlers = Record<string, () => Response>;

// Answers on the next tick and records how many calls overlapped.
function gated(handlers: Handlers) {
  let inflight = 0;
  const stats = { peak: 0, urls: [] as string[] };
  const fetchImpl = (async (url: string) => {
    stats.urls.push(url);
    inflight += 1;
    stats.peak = Math.max(stats.peak, inflight);
    await new Promise((r) => setTimeout(r, 0));
    inflight -= 1;
    for (const [needle, make] of Object.entries(handlers)) {
      if (url.includes(needle)) return make();
    }
    throw new Error(`unstubbed ${url}`);
  }) as unknown as typeof fetch;
  return { fetchImpl, stats };
}

const json = (body: unknown, status = 200) => () =>
  new Response(JSON.stringify(body), { status });

function healthy(): Handlers {
  return {
    "/world-storage/values": json({ data: [{ key: "k", value: 1 }], pagination: PAGE }),
    "/world-storage/env": json({ data: ["SECRET"], pagination: PAGE }),
    "/world-storage/players": json({ data: [ADDR], pagination: PAGE }),
    "/world-storage/usage/world": json({ usedBytes: 10, maxTotalSizeBytes: 100 }),
    "/names": json({ elements: [{ name: "petbarn" }] }),
    "/status": json({ ok: true }),
    "/world/petbarn.dcl.eth/about": json({ configurations: { scenesUrn: ["a", "b"] } }),
  };
}

describe("loadWorldsStorage", () => {
  it("issues the storage, names and realm reads in one stage", async () => {
    const { fetchImpl, stats } = gated(healthy());
    const data = await loadWorldsStorage(ADDR, { fetchImpl });

    expect(stats.peak).toBe(7);
    expect(stats.urls).toHaveLength(8);
    expect(data.source).toBe("live");
    expect(data.fallback).toBe(false);
    expect(data.values).toEqual([{ key: "k", value: 1 }]);
    expect(data.envKeys).toEqual([{ key: "SECRET" }]);
    expect(data.players.addresses).toEqual([ADDR]);
    expect(data.stats?.usedSpace).toBe(10);
    expect(data.worlds).toEqual([
      { name: "petbarn.dcl.eth", role: "owner", scenes: 2, usedBytes: 0, maxTotalSizeBytes: 0 },
    ]);
  });

  it("only flags fallback for the read that failed", async () => {
    const handlers = healthy();
    handlers["/world-storage/env"] = json({ error: "down" }, 500);
    const { fetchImpl } = gated(handlers);
    const data = await loadWorldsStorage(ADDR, { fetchImpl });

    expect(data.fallback).toBe(true);
    expect(data.source).toBe("live");
    expect(data.envKeys).toEqual([]);
    expect(data.values).toHaveLength(1);
    expect(data.worlds).toHaveLength(1);
  });
});

describe("loadManageWorlds", () => {
  it("combines directory counts for many worlds in the first request stage", async () => {
    const names = Array.from({ length: 30 }, (_, i) => `world${i}`);
    const { fetchImpl, stats } = gated({
      "/names": json({ elements: names.map((name) => ({ name })) }),
      "/status": json({ ok: true }),
      "/worlds?": json({ worlds: names.map((name, i) => ({ name: `${name}.dcl.eth`, deployed_scenes: i })) }),
    });
    const data = await loadManageWorlds(ADDR, undefined, { fetchImpl });
    expect(stats.peak).toBe(3);
    expect(stats.urls).toHaveLength(3);
    expect(data.worlds.map((world) => world.deployedScenes)).toEqual(names.map((_, i) => i));
    expect(stats.urls.some((url) => url.includes("/about"))).toBe(false);
  });

  it("resolves missing entries without dropping undeployed names or using another world's count", async () => {
    const { fetchImpl, stats } = gated({
      ...healthy(),
      "/worlds?": json({ worlds: [{ name: "someoneelse.dcl.eth", deployed_scenes: 99 }] }),
    });
    const data = await loadManageWorlds(ADDR, undefined, { fetchImpl });
    expect(data.worlds.map((world) => [world.name, world.deployedScenes])).toEqual([["petbarn.dcl.eth", 2]]);
    expect(stats.urls).toHaveLength(4);
  });

  it("falls back to world about when the directory is unavailable", async () => {
    const { fetchImpl, stats } = gated(healthy());
    const data = await loadManageWorlds(ADDR, undefined, { fetchImpl });

    expect(stats.peak).toBe(3);
    expect(stats.urls).toHaveLength(4);
    expect(data.worlds.map((w) => [w.name, w.deployedScenes])).toEqual([
      ["petbarn.dcl.eth", 2],
    ]);
  });
});
