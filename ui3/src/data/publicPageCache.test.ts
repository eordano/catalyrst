import { describe, expect, it, vi, afterEach } from "vitest";
import { createPublicPageCache } from "./publicPageCache";

afterEach(() => vi.useRealTimers());
describe("public browsing page cache", () => {
  it("shares a warmed page and coalesces simultaneous opens", async () => {
    const fetch = vi.fn(async (key: string) => [key]);
    const cache = createPublicPageCache(fetch);
    await Promise.all([cache.load("worlds"), cache.load("worlds")]);
    await cache.load("worlds");
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(cache.get("worlds").data).toEqual(["worlds"]);
    expect(cache.get("events").data).toBeUndefined();
  });
  it("retains usable results on refresh failure but expires them after five minutes", async () => {
    vi.useFakeTimers();
    const fetch = vi.fn().mockResolvedValueOnce(["world"]).mockRejectedValue(new Error("offline"));
    const cache = createPublicPageCache<string[]>(fetch);
    await cache.load("worlds");
    const updatedAt = cache.get("worlds").updatedAt;
    vi.advanceTimersByTime(31_000);
    await cache.load("worlds");
    expect(cache.get("worlds")).toMatchObject({ data: ["world"], error: true, updatedAt });
    vi.advanceTimersByTime(270_000);
    await cache.load("worlds");
    expect(cache.get("worlds")).toMatchObject({ data: undefined, error: true, updatedAt });
  });
});
