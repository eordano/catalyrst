import { afterEach, describe, expect, it, vi } from "vitest";

import {
  bytesFromString,
  findWorldSize,
  formatBytes,
  liveUsersFor,
  parseWcsWorlds,
  toManagedWorld,
  wcsBase,
  WcsWorldRowSchema,
  LiveDataSchema,
} from "./wcs";
import {
  loadLiveData,
  loadMyWorlds,
  loadPlatformStatus,
  loadWalletStats,
} from "./wcs.server";

const BASE = "https://worlds-content-server.example.test";

function jsonFetch(status: number, body: unknown): typeof fetch {
  return (async () =>
    new Response(status === 204 ? null : JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    })) as unknown as typeof fetch;
}

const WORLD_ROW = {
  name: "041.dcl.eth",
  owner: "0x37b323dd852e38114933f25ad53d0c04ec4ec2bd",
  title: "Ultimate Game Party",
  description: "Template scene with SDK7 for a 4-parcel area",
  content_rating: null,
  spawn_coordinates: "0,0",
  last_deployed_at: "2023-09-06T20:13:48.672Z",
  blocked_since: null,
  deployed_scenes: 1,
  thumbnail_hash: "bafkreidj26",
};

afterEach(() => {
  vi.unstubAllEnvs();
});

describe("wcsBase", () => {
  it("defaults to worlds-content-server.decentraland.org", () => {
    expect(wcsBase()).toBe("https://worlds-content-server.decentraland.org");
  });

  it("never resolves to a worlds. subdomain of catalyst.example.com", () => {
    // `worldsBase()` in client.ts rewrites catalystBase()'s hostname to
    // worlds.<domain>, which 404s every path used here. wcsBase must not.
    vi.stubEnv("CATALYST_URL", "https://catalyst.example.com");
    const host = new URL(wcsBase()).hostname;
    expect(host).not.toBe("worlds.example.com");
    expect(host.endsWith("catalyst.example.com")).toBe(false);
  });

  it("honours an explicit override and strips its trailing slash", () => {
    expect(wcsBase("https://wcs.example.test/")).toBe("https://wcs.example.test");
  });
});

describe("the wcs world row is snake_case and must be adapted, not coerced", () => {
  it("keeps deployed_scenes, last_deployed_at and blocked_since", () => {
    const row = WcsWorldRowSchema.parse(WORLD_ROW);
    const world = toManagedWorld(row, BASE);
    expect(world.deployedScenes).toBe(1);
    expect(world.lastDeployedAt).toBe("2023-09-06T20:13:48.672Z");
    expect(world.blockedSince).toBeNull();
    expect(world.thumbnail).toBe(`${BASE}/contents/bafkreidj26`);
  });

  it("drops only the rows that do not parse", () => {
    const worlds = parseWcsWorlds(
      { total: 2, worlds: [WORLD_ROW, { nope: true }] },
      BASE,
    );
    expect(worlds.map((w) => w.name)).toEqual(["041.dcl.eth"]);
  });
});

describe("loadMyWorlds", () => {
  it("a 200 with no rows is a real answer: live with an empty list", async () => {
    const d = await loadMyWorlds("0xabc", {
      base: BASE,
      fetchImpl: jsonFetch(200, { worlds: [], total: 0 }),
    });
    expect(d.state).toBe("live");
    if (d.state !== "live") throw new Error("unreachable");
    expect(d.value.worlds).toEqual([]);
    expect(d.value.total).toBe(0);
  });

  it("a 500 yields unavailable with the endpoint in the reason and NO value key", async () => {
    const d = await loadMyWorlds("0xabc", {
      base: BASE,
      fetchImpl: jsonFetch(500, { message: "boom" }),
    });
    expect(d.state).toBe("unavailable");
    expect(Object.keys(d)).not.toContain("value");
    if (d.state !== "unavailable") throw new Error("unreachable");
    expect(d.status).toBe(500);
    expect(d.reason).toContain("worlds-content-server.example.test/worlds");
    expect(d.reason).toContain("authorized_deployer=0xabc");
    expect(d.reason).toContain("Showing no value rather than a guess.");
  });

  it("an unreachable host yields unavailable with a null status, never []", async () => {
    const d = await loadMyWorlds("0xabc", {
      base: BASE,
      fetchImpl: (async () => {
        throw new Error("ECONNREFUSED");
      }) as unknown as typeof fetch,
    });
    expect(d.state).toBe("unavailable");
    if (d.state !== "unavailable") throw new Error("unreachable");
    expect(d.status).toBeNull();
    expect(d.reason).toContain("did not respond");
  });

  it("asks for the sort and page size the screen claims", async () => {
    const seen: string[] = [];
    const spy = (async (url: string) => {
      seen.push(url);
      return new Response(JSON.stringify({ worlds: [], total: 0 }), { status: 200 });
    }) as unknown as typeof fetch;
    await loadMyWorlds("0xABC", { base: BASE, fetchImpl: spy });
    expect(seen[0]).toContain("authorized_deployer=0xabc");
    expect(seen[0]).toContain("limit=100");
    expect(seen[0]).toContain("sort=last_deployed_at");
    expect(seen[0]).toContain("order=desc");
  });
});

describe("loadWalletStats", () => {
  const STATS = {
    wallet: "0x37b3",
    dclNames: [{ name: "041.dcl.eth", size: "6460699" }],
    ensNames: [],
    usedSpace: "6460699",
    maxAllowedSpace: "104857600",
  };

  it("returns live and keeps the byte counts as strings for BigInt parsing", async () => {
    const d = await loadWalletStats("0x37b3", {
      base: BASE,
      fetchImpl: jsonFetch(200, STATS),
    });
    expect(d.state).toBe("live");
    if (d.state !== "live") throw new Error("unreachable");
    expect(bytesFromString(d.value.usedSpace)).toBe(6460699n);
    expect(findWorldSize(d.value, "041.DCL.ETH")).toBe(6460699n);
  });

  it("degrades a 404 to unavailable", async () => {
    const d = await loadWalletStats("0x0", {
      base: BASE,
      fetchImpl: jsonFetch(404, { message: "nope" }),
    });
    expect(d.state).toBe("unavailable");
  });
});

describe("byte handling", () => {
  it("parses past Number.MAX_SAFE_INTEGER", () => {
    expect(bytesFromString("9007199254740993")).toBe(9007199254740993n);
  });
  it("returns null for junk rather than 0", () => {
    expect(bytesFromString("")).toBeNull();
    expect(bytesFromString("1.5")).toBeNull();
    expect(bytesFromString(null)).toBeNull();
    expect(formatBytes(null)).toBeNull();
  });
  it("formats", () => {
    expect(formatBytes(512n)).toBe("512 B");
    expect(formatBytes(6460699n)).toBe("6.2 MB");
  });
});

describe("loadLiveData / loadPlatformStatus", () => {
  const LIVE = {
    data: {
      totalUsers: 5,
      perWorld: [
        { worldName: "petbarn.dcl.eth", users: 3 },
        { worldName: "pokerclub.dcl.eth", users: 1 },
      ],
    },
    lastUpdated: "2026-08-01T09:52:30.775Z",
  };

  it("parses the live-data envelope", async () => {
    const d = await loadLiveData({ base: BASE, fetchImpl: jsonFetch(200, LIVE) });
    expect(d.state).toBe("live");
    if (d.state !== "live") throw new Error("unreachable");
    expect(d.value.data.totalUsers).toBe(5);
    expect(liveUsersFor(d.value, "PETBARN.DCL.ETH")).toBe(3);
    // Not listed is not the same as measured-zero; the caller decides.
    expect(liveUsersFor(d.value, "elsewhere.dcl.eth")).toBeNull();
  });

  it("refuses a missing perWorld array instead of inventing an empty one", () => {
    // This used to assert `perWorld === []`, because the schema laundered the
    // absence with `.nullish().transform((v) => v ?? [])`. That made safeParse
    // incapable of failing on this field, so a malformed body arrived in the UI
    // as a confident, live zero. The absence is a parse error now.
    const r = LiveDataSchema.safeParse({ data: { totalUsers: 0 }, lastUpdated: null });
    expect(r.success).toBe(false);
  });

  it("degrades a live-data body whose perWorld array is missing", async () => {
    const d = await loadLiveData({
      base: BASE,
      fetchImpl: jsonFetch(200, { data: { totalUsers: 0 }, lastUpdated: null }),
    });
    // `unavailable`, not `live` with zero users. Nobody measured a zero here; the
    // body could not be read, and those are different things to show a creator.
    expect(d.state).toBe("unavailable");
  });

  it("degrades a status read that returns HTML", async () => {
    const html = (async () =>
      new Response("<!doctype html><html></html>", { status: 200 })) as unknown as typeof fetch;
    const d = await loadPlatformStatus({ base: BASE, fetchImpl: html });
    expect(d.state).toBe("unavailable");
  });

  it("parses the status envelope", async () => {
    const d = await loadPlatformStatus({
      base: BASE,
      fetchImpl: jsonFetch(200, {
        content: { commitHash: "66fe4f", worldsCount: { ens: 119, dcl: 1432 } },
        comms: { adapterType: "livekit", rooms: 3, users: 5 },
      }),
    });
    expect(d.state).toBe("live");
    if (d.state !== "live") throw new Error("unreachable");
    expect(d.value.content.worldsCount.dcl).toBe(1432);
  });
});
