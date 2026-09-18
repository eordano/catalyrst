import { describe, expect, it, vi, beforeEach } from "vitest";
import { getJSON } from "../client";
import { fetchSceneHistory, fetchWorldHistory } from "../places/presence-history";
import { loadCreatorSceneStats, sampledPeaks } from "./scene-analytics.server";
import { parseCreatorScenesStats } from "./scene-analytics.gen";

vi.mock("../client", () => ({ getJSON: vi.fn() }));
vi.mock("../places/presence-history", () => ({ fetchSceneHistory: vi.fn(), fetchWorldHistory: vi.fn() }));
const wallet = `0x${"1".repeat(40)}`;
beforeEach(() => vi.resetAllMocks());

it("keeps unmeasured dates unknown and combines simultaneous realms before computing peaks", () => {
  const value = sampledPeaks([
    { taken_at: "2026-09-17T12:00:00Z", count: 2 },
    { taken_at: "2026-09-17T12:00:00Z", count: 3 },
    { taken_at: "2026-09-17T13:00:00Z", count: 4 },
    { taken_at: "2026-09-16T12:00:00Z", count: 0 },
    { taken_at: "2026-09-18T12:00:00Z", count: 999 },
  ], "2026-09-18");
  expect(value.windows.yesterday.peak_concurrent_users).toBe(5);
  expect(value.daily).toEqual([{ date: "2026-09-16", peak_concurrent_users: 0 }, { date: "2026-09-17", peak_concurrent_users: 5 }]);
  expect(sampledPeaks([], "2026-09-18").windows.last_30d.peak_concurrent_users).toBeNull();
});

describe("creator activity source", () => {
  it("returns a valid empty portfolio only after fetching the creator's actual places", async () => {
    vi.mocked(getJSON).mockResolvedValue({ data: [], total: 0 });
    expect(parseCreatorScenesStats(await loadCreatorSceneStats(wallet)).scenes).toEqual([]);
    expect(getJSON).toHaveBeenCalledWith("/places/api/places", expect.objectContaining({ query: { creator_address: wallet, limit: 100, offset: 0 } }));
  });
  it("loads World samples and leaves session metrics unmeasured", async () => {
    vi.mocked(getJSON).mockResolvedValue({ data: [{ creator_address: wallet, world: true, world_name: "audit.dcl.eth" }], total: 1 });
    vi.mocked(fetchWorldHistory).mockResolvedValue([{ world_name: "audit.dcl.eth", taken_at: new Date(Date.now() - 86400000).toISOString(), count: 1, live_users: 7 }]);
    const data = parseCreatorScenesStats(await loadCreatorSceneStats(wallet));
    expect(data.scenes[0].windows.last_7d).toMatchObject({ peakConcurrentUsers: 7, visits: null, users: null, medianActiveTimeS: null });
    expect(fetchSceneHistory).not.toHaveBeenCalled();
  });
  it("does not report empty success if the inventory or history service fails", async () => {
    vi.mocked(getJSON).mockRejectedValue(new Error("offline"));
    await expect(loadCreatorSceneStats(wallet)).rejects.toThrow("offline");
    vi.mocked(getJSON).mockResolvedValue({ data: [{ creator_address: wallet, world: false, base_position: "0,0" }], total: 1 });
    vi.mocked(fetchSceneHistory).mockRejectedValue(new Error("history offline"));
    await expect(loadCreatorSceneStats(wallet)).rejects.toThrow("history offline");
  });
  it("rejects an upstream response belonging to another wallet", async () => {
    vi.mocked(getJSON).mockResolvedValue({ data: [{ creator_address: `0x${"2".repeat(40)}`, world: false, base_position: "0,0" }], total: 1 });
    await expect(loadCreatorSceneStats(wallet)).rejects.toThrow("different creator");
  });
});
