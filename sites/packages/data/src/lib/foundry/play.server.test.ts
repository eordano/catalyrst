import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { sceneEmbed } from "./play.server";
import type { FoundryScene } from "./types";

/*
 * The reachability probe is memoised per realm with stale-while-revalidate:
 * a warm memo answers without touching the network; a stale one answers
 * immediately with the old result (and its old, honest probedAt) while one —
 * and only one — background refresh replaces it; concurrent cold visitors
 * share a single in-flight probe. Only Date is faked so AbortSignal.timeout
 * and vi.waitFor keep real timers; each test uses its own worldName because
 * the memo is module-level.
 */

const URN =
  "urn:decentraland:entity:bafy123?x=1&baseUrl=https://cdn.example/contents/";

function scene(worldName: string): FoundryScene {
  return {
    id: `scene-${worldName}`,
    title: worldName,
    worldName,
    entityId: null,
    deployedAt: null,
    sizeBytes: null,
    parcels: null,
    repoPath: null,
    botManifest: null,
    source: "worlds" as FoundryScene["source"],
    sourceNote: "",
    gddDocId: null,
    importedAt: null,
    description: null,
    thumbnailUrl: null,
    marketCell: null,
  };
}

let headStatus: number;
let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  headStatus = 200;
  fetchMock = vi.fn(async (input: unknown) => {
    const url = String(input);
    if (url.endsWith("/about")) {
      return new Response(
        JSON.stringify({ healthy: true, configurations: { scenesUrn: [URN] } }),
        { status: 200 },
      );
    }
    return new Response(null, { status: headStatus });
  });
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("sceneEmbed probe memo", () => {
  it("probes a cold realm once (about + HEAD) and stamps the probe's own time", async () => {
    const s = scene("w-cold");
    const t0 = Date.now();
    const embed = await sceneEmbed(s);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(embed?.reachable).toBe(true);
    expect(embed?.status).toBe(200);
    expect(embed?.probedAt).toBe(new Date(t0).toISOString());

    // Warm repeat inside the window: no network, and probedAt does not drift
    // to the render time.
    vi.setSystemTime(t0 + 60_000);
    const again = await sceneEmbed(s);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(again?.probedAt).toBe(embed?.probedAt);
  });

  it("answers a stale memo immediately and refreshes it in the background", async () => {
    const s = scene("w-stale");
    const t0 = Date.now();
    const cold = await sceneEmbed(s);
    expect(cold?.reachable).toBe(true);
    expect(fetchMock).toHaveBeenCalledTimes(2);

    headStatus = 404;
    vi.setSystemTime(t0 + 11 * 60_000);

    // The stale answer is immediate and honest: the old result under the old
    // probe stamp, never the old result under a fresh-looking time.
    const stale = await sceneEmbed(s);
    expect(stale?.reachable).toBe(true);
    expect(stale?.probedAt).toBe(cold?.probedAt);

    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(4));
    const fresh = await vi.waitFor(async () => {
      const next = await sceneEmbed(s);
      expect(next?.reachable).toBe(false);
      return next;
    });
    expect(fresh?.status).toBe(404);
    expect(fresh?.probedAt).toBe(new Date(t0 + 11 * 60_000).toISOString());
    // The settled memo stops the refreshing: still exactly one refresh.
    expect(fetchMock).toHaveBeenCalledTimes(4);
  });

  it("deduplicates two concurrent cold probes into one flight", async () => {
    const s = scene("w-concurrent");
    const [a, b] = await Promise.all([sceneEmbed(s), sceneEmbed(s)]);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(a?.probedAt).toBe(b?.probedAt);
    expect(a?.reachable).toBe(true);
    expect(b?.reachable).toBe(true);
  });
});
