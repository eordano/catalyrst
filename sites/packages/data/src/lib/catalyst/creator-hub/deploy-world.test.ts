import { describe, expect, it } from "vitest";

import {
  fetchWorldsRealm,
  isPersonalWorld,
  personalWorldName,
  withPersonalWorld,
} from "./deploy-world";
import { fallbackDeployWorld, loadDeployWorld } from "./deploy-world.server";
import { loadManageWorlds } from "./manage-worlds.server";

const ADDR = "0x4d02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa7bf2";
const PERSONAL = `${ADDR}.dcl.eth`;

function stub(handlers: Record<string, () => Response>): typeof fetch {
  return (async (url: string) => {
    for (const [needle, make] of Object.entries(handlers)) {
      if (url.includes(needle)) return make();
    }
    throw new Error(`unstubbed ${url}`);
  }) as unknown as typeof fetch;
}

const json = (body: unknown, status = 200) => () =>
  new Response(JSON.stringify(body), { status });

const realmStatus = (personalWorlds: boolean) =>
  json({
    content: { commitHash: "abc", worldsCount: { ens: 0, dcl: 1 } },
    comms: {
      adapterType: "livekit",
      statusUrl: "https://lk.example/",
      rooms: 0,
      users: 0,
      timestamp: 0,
    },
    personalWorlds: personalWorlds ? { maxWorlds: 100, maxSizeBytes: 52428800 } : null,
  });

describe("personalWorldName", () => {
  it("is the lowercase wallet address as a .dcl.eth name, never a NAME label, and never exists for a bad address", () => {
    expect(personalWorldName(ADDR.toUpperCase().replace("0X", "0x"))).toBe(PERSONAL);
    expect(personalWorldName(` ${ADDR} `)).toBe(PERSONAL);
    expect(isPersonalWorld(PERSONAL)).toBe(true);
    expect(isPersonalWorld(PERSONAL.toUpperCase())).toBe(true);
    expect(personalWorldName("")).toBeNull();
    expect(personalWorldName(null)).toBeNull();
    expect(personalWorldName("0x4d02")).toBeNull();
    expect(isPersonalWorld("boedo.dcl.eth")).toBe(false);
    expect(isPersonalWorld("0x4d02.dcl.eth")).toBe(false);
  });

  it("appends the personal world after owned NAMEs, once, only when the realm offers it", () => {
    const owned = [{ name: "boedo.dcl.eth", provider: "dcl" as const, world: null }];
    const names = withPersonalWorld(ADDR, owned, true);
    expect(names.map((n) => n.name)).toEqual(["boedo.dcl.eth", PERSONAL]);
    expect(withPersonalWorld(ADDR, names, true).map((n) => n.name)).toEqual([
      "boedo.dcl.eth",
      PERSONAL,
    ]);
    expect(withPersonalWorld("", owned, true)).toEqual(owned);
    expect(withPersonalWorld(ADDR, owned, false)).toEqual(owned);
  });
});

describe("fetchWorldsRealm", () => {
  it("reads the personal-world policy from /status, treats another shape as online without it, and rejects a 503", async () => {
    await expect(
      fetchWorldsRealm({ fetchImpl: stub({ "/status": realmStatus(true) }) }),
    ).resolves.toEqual({ online: true, personalWorlds: true });
    await expect(
      fetchWorldsRealm({ fetchImpl: stub({ "/status": realmStatus(false) }) }),
    ).resolves.toEqual({ online: true, personalWorlds: false });
    await expect(
      fetchWorldsRealm({ fetchImpl: stub({ "/status": json({ ok: true }) }) }),
    ).resolves.toEqual({ online: true, personalWorlds: false });
    await expect(
      fetchWorldsRealm({ fetchImpl: stub({ "/status": json({ error: "down" }, 503) }) }),
    ).rejects.toThrow();
  });
});

describe("loadDeployWorld", () => {
  it("offers the personal test world on a hosting realm, after an owned NAME when there is one", async () => {
    const data = await loadDeployWorld(ADDR, {
      fetchImpl: stub({
        "/names": json({ elements: [], totalAmount: 0 }),
        "/status": realmStatus(true),
      }),
    });
    expect(data.source).toBe("live");
    expect(data.liveEmpty).toBe(false);
    expect(data.personalWorlds).toBe(true);
    expect(data.names.map((n) => n.name)).toEqual([PERSONAL]);
    expect(data.worldsOnline).toBe(true);

    const owned = await loadDeployWorld(ADDR, {
      fetchImpl: stub({
        "/names": json({ elements: [{ name: "boedo" }], totalAmount: 1 }),
        "/status": realmStatus(true),
      }),
    });
    expect(owned.names.map((n) => n.name)).toEqual(["boedo.dcl.eth", PERSONAL]);
  });

  it("offers nothing on a realm without personal worlds or while the realm is unreachable", async () => {
    const data = await loadDeployWorld(ADDR, {
      fetchImpl: stub({
        "/names": json({ elements: [], totalAmount: 0 }),
        "/status": realmStatus(false),
      }),
    });
    expect(data.liveEmpty).toBe(true);
    expect(data.personalWorlds).toBe(false);
    expect(data.names).toEqual([]);
    expect(data.worldsOnline).toBe(true);

    const down = await loadDeployWorld(ADDR, {
      fetchImpl: stub({
        "/names": json({ elements: [], totalAmount: 0 }),
        "/status": json({ error: "down" }, 503),
      }),
    });
    expect(down.worldsOnline).toBe(false);
    expect(down.personalWorlds).toBe(false);
    expect(down.liveEmpty).toBe(true);
    expect(fallbackDeployWorld(ADDR).names).toEqual([]);
    expect(fallbackDeployWorld(ADDR).liveEmpty).toBe(true);
  });

  it("still offers the personal world when the names lookup fails, and has no target at all without a wallet", async () => {
    const data = await loadDeployWorld(ADDR, {
      fetchImpl: stub({
        "/names": json({ error: "boom" }, 500),
        "/status": realmStatus(true),
      }),
    });
    expect(data.source).toBe("empty");
    expect(data.liveEmpty).toBe(false);
    expect(data.names.map((n) => n.name)).toEqual([PERSONAL]);

    const anonymous = await loadDeployWorld(null, {
      fetchImpl: stub({ "/status": realmStatus(true) }),
    });
    expect(anonymous.liveEmpty).toBe(true);
    expect(anonymous.names).toEqual([]);
    expect(fallbackDeployWorld("").liveEmpty).toBe(true);
  });
});

describe("loadManageWorlds", () => {
  it("lists the personal world next to the owned NAMEs only when the realm hosts them", async () => {
    const data = await loadManageWorlds(ADDR, undefined, {
      fetchImpl: stub({
        "/names": json({ elements: [{ name: "boedo" }] }),
        "/status": realmStatus(true),
        [`/world/${encodeURIComponent(PERSONAL)}/about`]: json(
          { configurations: { scenesUrn: ["urn:1", "urn:2"] } },
        ),
        "/world/boedo.dcl.eth/about": json({ error: "not found" }, 404),
      }),
    });
    const byName = Object.fromEntries(data.worlds.map((w) => [w.name, w]));
    expect(Object.keys(byName).sort()).toEqual([PERSONAL, "boedo.dcl.eth"].sort());
    expect(byName[PERSONAL].deployedScenes).toBe(2);
    expect(byName[PERSONAL].owner).toBe(ADDR);
    expect(byName["boedo.dcl.eth"].deployedScenes).toBe(0);

    const plain = await loadManageWorlds(ADDR, undefined, {
      fetchImpl: stub({
        "/names": json({ elements: [{ name: "boedo" }] }),
        "/status": realmStatus(false),
        "/world/boedo.dcl.eth/about": json({ error: "not found" }, 404),
      }),
    });
    expect(plain.worlds.map((w) => w.name)).toEqual(["boedo.dcl.eth"]);
  });
});
