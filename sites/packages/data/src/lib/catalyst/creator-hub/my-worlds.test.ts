import { describe, expect, it } from "vitest";

import { loadMyWorldsUnion, unionWorlds } from "./my-worlds.server";
import type { ManagedWorld } from "./manage-worlds";

const WCS = "https://wcs.example.test";

function upstream(name: string, deployedScenes = 1): ManagedWorld {
  return {
    name,
    owner: "0xabc",
    title: null,
    description: null,
    contentRating: null,
    spawnCoordinates: null,
    lastDeployedAt: "2026-07-12T00:00:00.000Z",
    blockedSince: null,
    deployedScenes,
    thumbnail: null,
    role: "owner",
  };
}

describe("unionWorlds", () => {
  it("marks where every row came from and never merges the two into one claim", () => {
    const rows = unionWorlds(
      [{ name: "petbarn", contractAddress: null, tokenId: null }],
      [upstream("petbarn.dcl.eth"), upstream("elsewhere.dcl.eth")],
    );
    const byName = Object.fromEntries(rows.map((r) => [r.name, r.origin]));
    expect(byName["petbarn.dcl.eth"]).toBe("both");
    expect(byName["elsewhere.dcl.eth"]).toBe("upstream");
  });

  it("keeps a NAME with nothing deployed as a catalyst.example.com-only row", () => {
    const rows = unionWorlds(
      [{ name: "onlyname.dcl.eth", contractAddress: null, tokenId: null }],
      [],
    );
    expect(rows).toHaveLength(1);
    expect(rows[0].origin).toBe("catalyst.example.com");
    expect(rows[0].name).toBe("onlyname.dcl.eth");
  });
});

describe("loadMyWorldsUnion", () => {
  function stub(handlers: Record<string, () => Response>): typeof fetch {
    return (async (url: string) => {
      for (const [needle, make] of Object.entries(handlers)) {
        if (url.includes(needle)) return make();
      }
      throw new Error(`unstubbed ${url}`);
    }) as unknown as typeof fetch;
  }

  const wcsOk = () =>
    new Response(
      JSON.stringify({
        total: 1,
        worlds: [
          {
            name: "petbarn.dcl.eth",
            owner: "0xabc",
            title: "Pet Barn",
            description: null,
            content_rating: null,
            spawn_coordinates: "0,0",
            last_deployed_at: "2026-07-12T00:00:00.000Z",
            blocked_since: null,
            deployed_scenes: 1,
            thumbnail_hash: null,
          },
        ],
      }),
      { status: 200 },
    );

  const namesOk = () =>
    new Response(JSON.stringify({ elements: [{ name: "otherworld" }] }), {
      status: 200,
    });

  it("unions both hosts when both answer", async () => {
    const data = await loadMyWorldsUnion("0xABC", {
      wcsBase: WCS,
      fetchImpl: stub({ "/names": namesOk, "/worlds": wcsOk }),
    });
    expect(data.bothFailed).toBe(false);
    expect(data.partial).toBe(false);
    expect(data.rows.map((r) => r.origin).sort()).toEqual(["catalyst.example.com", "upstream"]);
  });

  it("renders the surviving host's rows and flags the list as incomplete", async () => {
    const data = await loadMyWorldsUnion("0xABC", {
      wcsBase: WCS,
      fetchImpl: stub({
        "/names": () => new Response("boom", { status: 500 }),
        "/worlds": wcsOk,
      }),
    });
    expect(data.partial).toBe(true);
    expect(data.bothFailed).toBe(false);
    expect(data.rows).toHaveLength(1);
    expect(data.dclOne.state).toBe("unavailable");
    if (data.dclOne.state !== "unavailable") throw new Error("unreachable");
    expect(data.dclOne.reason).toContain("/lambdas/users/0xabc/names");
  });

  it("when both hosts fail there are no rows and both datums say so", async () => {
    const data = await loadMyWorldsUnion("0xABC", {
      wcsBase: WCS,
      fetchImpl: (async () => {
        throw new Error("down");
      }) as unknown as typeof fetch,
    });
    expect(data.bothFailed).toBe(true);
    expect(data.rows).toEqual([]);
    expect(data.dclOne.state).toBe("unavailable");
    expect(data.upstream.state).toBe("unavailable");
    expect(Object.keys(data.upstream)).not.toContain("value");
  });
});
