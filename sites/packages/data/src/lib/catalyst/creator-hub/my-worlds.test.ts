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
  it("marks where every row came from, never merges the two into one claim, and keeps a NAME with nothing deployed as a catalyst.example.com-only row", () => {
    const rows = unionWorlds(
      [{ name: "petbarn", contractAddress: null, tokenId: null }],
      [upstream("petbarn.dcl.eth"), upstream("elsewhere.dcl.eth")],
    );
    const byName = Object.fromEntries(rows.map((r) => [r.name, r.origin]));
    expect(byName["petbarn.dcl.eth"]).toBe("both");
    expect(byName["elsewhere.dcl.eth"]).toBe("upstream");

    const nameOnly = unionWorlds(
      [{ name: "onlyname.dcl.eth", contractAddress: null, tokenId: null }],
      [],
    );
    expect(nameOnly).toHaveLength(1);
    expect(nameOnly[0].origin).toBe("catalyst.example.com");
    expect(nameOnly[0].name).toBe("onlyname.dcl.eth");
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

  it("unions both hosts when both answer, flags the list as incomplete when one fails, and has no rows when both fail", async () => {
    const both = await loadMyWorldsUnion("0xABC", {
      wcsBase: WCS,
      fetchImpl: stub({ "/names": namesOk, "/worlds": wcsOk }),
    });
    expect(both.bothFailed).toBe(false);
    expect(both.partial).toBe(false);
    expect(both.rows.map((r) => r.origin).sort()).toEqual(["catalyst.example.com", "upstream"]);

    const partial = await loadMyWorldsUnion("0xABC", {
      wcsBase: WCS,
      fetchImpl: stub({
        "/names": () => new Response("boom", { status: 500 }),
        "/worlds": wcsOk,
      }),
    });
    expect(partial.partial).toBe(true);
    expect(partial.bothFailed).toBe(false);
    expect(partial.rows).toHaveLength(1);
    expect(partial.dclOne.state).toBe("unavailable");
    if (partial.dclOne.state !== "unavailable") throw new Error("unreachable");
    expect(partial.dclOne.reason).toContain("/lambdas/users/0xabc/names");

    const failed = await loadMyWorldsUnion("0xABC", {
      wcsBase: WCS,
      fetchImpl: (async () => {
        throw new Error("down");
      }) as unknown as typeof fetch,
    });
    expect(failed.bothFailed).toBe(true);
    expect(failed.rows).toEqual([]);
    expect(failed.dclOne.state).toBe("unavailable");
    expect(failed.upstream.state).toBe("unavailable");
    expect(Object.keys(failed.upstream)).not.toContain("value");
  });
});
