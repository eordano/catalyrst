import { afterEach, expect, it, vi } from "vitest";
import { PUBLIC_FEED_MAX_AGE_MS, publicFeedMemo } from "./public-feed.server";

afterEach(() => vi.useRealTimers());

it("returns cached content immediately, coalesces refreshes, and preserves its original age", async () => {
  vi.useFakeTimers();
  let finish!: (value: string) => void;
  const load = vi.fn().mockResolvedValueOnce("old").mockImplementation(() => new Promise<string>(resolve => { finish = resolve; }));
  const feed = publicFeedMemo({ load, freshMs: 1_000 });
  const initial = await feed();
  vi.advanceTimersByTime(1_001);
  expect(await feed()).toEqual({ ...initial, refreshing: true });
  expect(await feed()).toEqual({ ...initial, refreshing: true });
  expect(load).toHaveBeenCalledTimes(2);
  finish("new");
  await vi.waitFor(async () => expect((await feed()).data).toBe("new"));
  expect((await feed()).updatedAt).toBeGreaterThan(initial.updatedAt);
});

it("reports refresh failure without discarding usable content, then refuses content older than the max age", async () => {
  vi.useFakeTimers();
  const load = vi.fn().mockResolvedValueOnce(["usable"]).mockRejectedValue(new Error("offline"));
  const feed = publicFeedMemo({ load, freshMs: 1_000 });
  await feed();
  vi.advanceTimersByTime(1_001);
  expect((await feed()).data).toEqual(["usable"]);
  await vi.waitFor(async () => expect((await feed()).refreshFailed).toBe(true));
  vi.advanceTimersByTime(PUBLIC_FEED_MAX_AGE_MS);
  await expect(feed()).rejects.toThrow("offline");
});

it("isolates keys and prevents a refresh started before reset from repopulating the cache", async () => {
  let finish!: (value: string) => void;
  const load = vi.fn().mockImplementationOnce(() => new Promise<string>(resolve => { finish = resolve; })).mockResolvedValue("current");
  const feed = publicFeedMemo({ keyOf: (key: string) => key, load });
  const old = feed("a");
  await Promise.resolve();
  feed.reset();
  expect((await feed("a")).data).toBe("current");
  finish("obsolete");
  await old;
  expect((await feed("a")).data).toBe("current");
  await feed("b");
  expect(load).toHaveBeenCalledTimes(3);
});
