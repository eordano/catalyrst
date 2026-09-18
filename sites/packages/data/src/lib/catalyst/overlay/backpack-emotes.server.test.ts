import { describe, expect, it, vi } from "vitest";

import { loadBackpackEmotes } from "./backpack-emotes.server";

const ADDR = `0x${"a".repeat(40)}`;
const URN = (i: number) => `urn:decentraland:matic:collections-v2:0xc:${i}`;

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function upstream(opts: { ownedStatus?: number } = {}) {
  const inflight = new Set<string>();
  let peak = 0;
  const fetchImpl = vi.fn(async (input: string | URL | Request) => {
    const url = String(input);
    inflight.add(url);
    peak = Math.max(peak, inflight.size);
    await new Promise((r) => setTimeout(r, 5));
    inflight.delete(url);
    if (url.includes("/emotes-by-owner/")) {
      return json(
        Array.from({ length: 45 }, (_, i) => ({ urn: URN(i) })),
        opts.ownedStatus ?? 200,
      );
    }
    if (url.includes("/lambdas/profiles/")) {
      return json({ avatars: [{ avatar: { emotes: [{ slot: 1, urn: URN(1) }] } }] });
    }
    if (url.includes("/lambdas/collections/emotes?")) {
      const ids = new URL(url).searchParams.getAll("emoteId");
      return json(
        ids.map((urn) => ({
          urn,
          name: "e",
          emoteDataADR74: { category: "dance", loop: false },
        })),
      );
    }
    throw new Error(`unexpected ${url}`);
  });
  return { fetchImpl: fetchImpl as unknown as typeof fetch, peak: () => peak };
}

describe("loadBackpackEmotes", () => {
  it("reads owned + profile in one stage and every def chunk in one stage", async () => {
    const up = upstream();
    const data = await loadBackpackEmotes(ADDR, { fetchImpl: up.fetchImpl });
    expect(up.peak()).toBeGreaterThanOrEqual(2);
    expect(data.source).toBe("live");
    expect(data.catalog).toHaveLength(45);
    expect(data.loadout).toEqual([{ slot: 1, urn: URN(1), name: "e" }]);
  });

  it("an unreadable owned list still reads the profile and flags the error", async () => {
    const up = upstream({ ownedStatus: 500 });
    const data = await loadBackpackEmotes(ADDR, { fetchImpl: up.fetchImpl });
    expect(data.source).toBe("error");
    expect(data.error).toBe(true);
    expect(data.catalog).toEqual([]);
    expect(data.loadout).toEqual([{ slot: 1, urn: URN(1), name: "e" }]);
  });
});
