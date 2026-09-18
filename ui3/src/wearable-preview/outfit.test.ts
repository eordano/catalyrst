import { afterEach, expect, test, vi } from "vitest";
import { fetchEntities } from "./outfit";

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

test("a stalled entity request times out and is evicted so retry fetches fresh metadata", async () => {
  vi.useFakeTimers();
  const fetch = vi.fn((_url: string, init: RequestInit) => new Promise((_resolve, reject) => {
    init.signal!.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")));
  }));
  vi.stubGlobal("fetch", fetch);
  const failed = expect(fetchEntities("https://avatar.test", ["urn:body"])).rejects.toThrow("Aborted");
  await vi.runAllTimersAsync();
  await failed;
  const entity = { pointers: ["urn:body"] };
  fetch.mockResolvedValueOnce({ ok: true, json: async () => [entity] });
  expect(await fetchEntities("https://avatar.test", ["urn:body"])).toEqual(new Map([["urn:body", entity]]));
  expect(fetch).toHaveBeenCalledTimes(2);
});

test("simultaneous previews share one metadata batch and reuse overlapping pointers", async () => {
  const fetch = vi.fn(async (_url: string, init: RequestInit) => ({
    ok: true,
    json: async () => (JSON.parse(String(init.body)).pointers as string[]).map(pointer => ({ pointers: [pointer.toUpperCase()] })),
  }));
  vi.stubGlobal("fetch", fetch);
  const previews = Array.from({ length: 10 }, (_, index) =>
    fetchEntities("https://grid.test", ["URN:BODY", `urn:outfit:${index}`]));
  const results = await Promise.all(previews);
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(JSON.parse(String(fetch.mock.calls[0]![1].body)).pointers).toHaveLength(11);
  results.forEach((result, index) => expect([...result.keys()]).toEqual(["urn:body", `urn:outfit:${index}`]));
  await fetchEntities("https://grid.test", ["urn:body", "urn:outfit:0"]);
  expect(fetch).toHaveBeenCalledTimes(1);
});

test("metadata batches isolate catalyst origins and respect the server pointer limit", async () => {
  const fetch = vi.fn(async (_url: string, init: RequestInit) => ({
    ok: true, json: async () => (JSON.parse(String(init.body)).pointers as string[]).map(pointer => ({ pointers: [pointer] })),
  }));
  vi.stubGlobal("fetch", fetch);
  const many = Array.from({ length: 1001 }, (_, index) => `urn:item:${index}`);
  const [first, second, empty] = await Promise.all([
    fetchEntities("https://first.test", many),
    fetchEntities("https://second.test", [many[0]!]),
    fetchEntities("https://empty.test", []),
  ]);
  expect(first.size).toBe(1001);
  expect(second.size).toBe(1);
  expect(empty.size).toBe(0);
  expect(fetch.mock.calls.map(([url, init]) => [url, JSON.parse(String(init.body)).pointers.length])).toEqual([
    ["https://first.test/content/entities/active", 1000],
    ["https://first.test/content/entities/active", 1],
    ["https://second.test/content/entities/active", 1],
  ]);
});

test("a failed batch releases every preview pointer for retry", async () => {
  const fetch = vi.fn().mockResolvedValueOnce({ ok: false, status: 503 });
  vi.stubGlobal("fetch", fetch);
  const first = fetchEntities("https://retry.test", ["urn:a"]);
  const second = fetchEntities("https://retry.test", ["urn:b"]);
  await expect(Promise.all([first, second])).rejects.toThrow("503");
  const entities = [{ pointers: ["urn:a", "urn:b"] }];
  fetch.mockResolvedValueOnce({ ok: true, json: async () => entities });
  expect((await fetchEntities("https://retry.test", ["urn:a", "urn:b"])).size).toBe(2);
  expect(fetch).toHaveBeenCalledTimes(2);
});
