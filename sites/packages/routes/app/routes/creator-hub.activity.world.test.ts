import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { loader } from "./creator-hub.activity_.$world";

const WORLD = "petbarn.dcl.eth";
const ADDRESS = "0x1111111111111111111111111111111111111111";
const SHOWABLE = new Set(["live", "sampled", "snapshot"]);

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const WCS_ROW = {
  name: WORLD,
  owner: "0x313d",
  title: "Pet Barn",
  description: null,
  content_rating: null,
  spawn_coordinates: "0,0",
  last_deployed_at: "2026-07-12T10:00:00Z",
  blocked_since: null,
  deployed_scenes: 1,
  thumbnail_hash: null,
};

type Routes = { match: (url: string) => boolean; reply: () => Response | never }[];

function stubFetch(routes: Routes) {
  vi.spyOn(globalThis, "fetch").mockImplementation((input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    for (const route of routes) {
      if (route.match(url)) return Promise.resolve(route.reply());
    }
    return Promise.resolve(json({ message: "not stubbed" }, 404));
  });
}

function presenceDownRoutes({ myWorlds = true } = {}): Routes {
  return [
    {
      match: (u) => u.includes("/presence/"),
      reply: () => {
        throw new Error("presence is unreachable");
      },
    },
    {
      match: (u) => u.includes("authorized_deployer"),
      reply: () => json(myWorlds ? { total: 1, worlds: [WCS_ROW] } : { total: 0, worlds: [] }),
    },
    { match: (u) => u.includes("/live-data"), reply: () => json({ data: { totalUsers: 3, perWorld: [{ worldName: WORLD, users: 3 }] }, lastUpdated: null }) },
    {
      match: (u) => u.includes("/wallet/") && u.includes("/stats"),
      reply: () =>
        json({
          wallet: ADDRESS,
          dclNames: [{ name: WORLD, size: "62700000" }],
          ensNames: [],
          usedSpace: "62700000",
          maxAllowedSpace: "7000000000",
          blockedSince: null,
        }),
    },
    {
      match: (u) => u.includes(`/world/`) && u.includes("/about"),
      reply: () =>
        json({
          healthy: true,
          acceptingUsers: true,
          spawnCoordinates: "0,0",
          configurations: { scenesUrn: ["urn:decentraland:entity:bafy123"], realmName: WORLD },
        }),
    },
    {
      match: (u) => u.includes("/permissions"),
      reply: () =>
        json({
          owner: "0x313d",
          permissions: {
            access: { type: "unrestricted", wallets: [] },
            deployment: { type: "allow-list", wallets: ["0xabc"] },
            streaming: { type: "allow-list", wallets: [] },
          },
        }),
    },
    {
      match: (u) => u.includes("/places/api/worlds"),
      reply: () =>
        json({
          data: [
            {
              id: "p1",
              world_name: WORLD,
              title: "Pet Barn",
              likes: 12,
              dislikes: 1,
              favorites: 4,
              like_rate: 0.92,
              deployed_at: "2026-07-12T10:00:00Z",
            },
          ],
          total: 1,
        }),
    },
    { match: (u) => u.endsWith("/about"), reply: () => json({ healthy: true, acceptingUsers: true, content: { synchronizationStatus: "Syncing" }, configurations: { realmName: "main" } }) },
  ];
}

function get(world: string, search = `?address=${ADDRESS}`) {
  return {
    request: new Request(
      `https://sites.test/creator-hub/activity/${encodeURIComponent(world)}${search}`,
    ),
    params: { world },
    context: {} as never,
  };
}

beforeEach(() => {
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /creator-hub/activity/:world", () => {
  it("404s a world unknown to both worlds-content-server and catalyst", async () => {
    stubFetch([
      { match: (u) => u.includes("authorized_deployer"), reply: () => json({ total: 0, worlds: [] }) },
      { match: () => true, reply: () => json({ message: "not found" }, 404) },
    ]);

    const res = await loader(get("ghost.dcl.eth") as never);

    expect(res.init?.status).toBe(404);
    const payload = res.data as Record<string, unknown>;
    expect(payload.notFound).toBe(true);
  });

  it("presence down + wcs up is a 200 with deploy and reception still showable", async () => {
    stubFetch(presenceDownRoutes());

    const res = await loader(get(WORLD) as never);

    expect(res.init?.status ?? 200).toBe(200);

    const payload = res.data as Record<string, unknown>;
    expect(payload.notFound).toBe(false);
    expect(payload.allUpstreamsDown).toBe(false);

    const state = (key: string) =>
      (payload[key] as Record<string, unknown>).state as string;

    expect(SHOWABLE.has(state("sceneUrn"))).toBe(true);
    expect(SHOWABLE.has(state("spawnCoordinates"))).toBe(true);
    expect(SHOWABLE.has(state("storage"))).toBe(true);
    expect(SHOWABLE.has(state("reception"))).toBe(true);
    expect(SHOWABLE.has(state("worldMeta"))).toBe(true);

    for (const key of ["inThisWorld", "history", "peak", "occupiedSnapshots"]) {
      const d = payload[key] as Record<string, unknown>;
      expect(SHOWABLE.has(d.state as string)).toBe(false);
      expect(Object.hasOwn(d, "value")).toBe(false);
    }
  });

  it("never renders the scene key-value store as a zero", async () => {
    stubFetch(presenceDownRoutes());

    const res = await loader(get(WORLD) as never);
    const payload = res.data as Record<string, unknown>;
    const kv = payload.sceneKvStorage as Record<string, unknown>;

    expect(kv.state).toBe("unavailable");
    expect(Object.hasOwn(kv, "value")).toBe(false);
    expect(String(kv.reason)).toMatch(/ADR-44/);
  });

  it("does not gate a world the caller does not deploy", async () => {
    stubFetch(presenceDownRoutes({ myWorlds: false }));

    const res = await loader(get(WORLD) as never);

    expect(res.init?.status ?? 200).toBe(200);
    const payload = res.data as Record<string, unknown>;
    expect(payload.notFound).toBe(false);
    expect(payload.deployedByCaller).toBe(false);
  });
});
