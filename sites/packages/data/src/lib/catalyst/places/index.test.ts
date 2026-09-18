import { describe, expect, it } from "vitest";

import { fetchProfilePlaces } from "./index";

describe("fetchProfilePlaces", () => {
  it("asks catalyrst-places for the creator's places only, in the default like_score order", async () => {
    const seen: string[] = [];
    const fetchImpl = (async (url: string) => {
      seen.push(url);
      return new Response(JSON.stringify({ ok: true, data: [], total: 0 }), { status: 200 });
    }) as unknown as typeof fetch;

    const out = await fetchProfilePlaces(
      "0x17A253C2ac0d5BA92cadBBF665e3390C9913dC5D",
      {},
      { fetchImpl },
    );

    expect(out).toEqual([]);
    expect(seen).toHaveLength(1);
    const url = new URL(seen[0]);
    expect(url.pathname).toBe("/places/api/places");
    expect(url.searchParams.get("creator_address")).toBe(
      "0x17a253c2ac0d5ba92cadbbf665e3390c9913dc5d",
    );
    expect(url.searchParams.get("limit")).toBe("100");
    expect(url.searchParams.has("order_by")).toBe(false);
  });
});
