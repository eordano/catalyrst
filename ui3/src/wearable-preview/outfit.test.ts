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
  await vi.advanceTimersByTimeAsync(20000);
  await failed;
  const entity = { pointers: ["urn:body"] };
  fetch.mockResolvedValueOnce({ ok: true, json: async () => [entity] });
  expect(await fetchEntities("https://avatar.test", ["urn:body"])).toEqual(new Map([["urn:body", entity]]));
  expect(fetch).toHaveBeenCalledTimes(2);
});
