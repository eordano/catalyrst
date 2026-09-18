import { afterEach, describe, expect, test, vi } from "vitest";

import { populateProjectRealm } from "./project-realm";

afterEach(() => {
  vi.unstubAllGlobals();
});

test("the preview owns its startup CRDT and script instead of relying on deployment URLs", async () => {
  const stored = new Map<string, Response>();
  vi.stubGlobal("caches", { open: async () => ({
    keys: async () => [...stored.keys()],
    delete: async (key: string) => stored.delete(key),
    put: async (key: string, response: Response) => stored.set(key, response),
  }) });
  const base = "https://preview.test/_project/content";
  vi.stubGlobal("fetch", vi.fn(async (url: string) => {
    if (url.endsWith("/about")) return Response.json({ content: { publicUrl: base } });
    if (url.endsWith("/entities/active")) return Response.json([{
      metadata: { main: "bin/index.js" },
      content: [{ file: "main.crdt", hash: "old-crdt" }, { file: "bin/index.js", hash: "old-script" }],
    }]);
    if (url.endsWith("/old-crdt")) return new Response(new Uint8Array([1, 2, 3]));
    if (url.endsWith("/old-script")) return new Response("module.exports = {};");
    throw new Error(`Unexpected request ${url}`);
  }));
  expect(await populateProjectRealm({})).toMatchObject({ ok: true, assets: 0 });
  expect(new Uint8Array(await stored.get(`${base}/contents/old-crdt`)!.arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]));
  expect(await stored.get(`${base}/contents/old-script`)!.text()).toBe("module.exports = {};");
});

test("missing template startup content leaves the previous preview intact", async () => {
  const put = vi.fn(), remove = vi.fn();
  vi.stubGlobal("caches", { open: async () => ({ keys: async () => ["old"], delete: remove, put }) });
  vi.stubGlobal("fetch", vi.fn(async (url: string) => {
    if (url.endsWith("/about")) return Response.json({ content: { publicUrl: "https://preview.test/_project/content" } });
    if (url.endsWith("/entities/active")) return Response.json([{
      metadata: { main: "bin/index.js" }, content: [{ file: "main.crdt", hash: "missing-crdt" }],
    }]);
    return new Response("missing", { status: 404 });
  }));
  expect(await populateProjectRealm({})).toEqual({ ok: false, reason: "template-content-unreachable" });
  expect(remove).not.toHaveBeenCalled();
  expect(put).not.toHaveBeenCalled();
});

describe("populateProjectRealm cancellation", () => {
  test("aborts the in-flight realm request before opening the cache", async () => {
    const controller = new AbortController();
    const open = vi.fn();
    vi.stubGlobal("caches", { open });
    let received: AbortSignal | undefined;
    vi.stubGlobal("fetch", vi.fn((_: string, init?: RequestInit) => {
      received = init?.signal ?? undefined;
      return new Promise<Response>((_, reject) => {
        init?.signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true });
      });
    }));

    const pending = populateProjectRealm({}, null, { signal: controller.signal });
    await vi.waitFor(() => expect(received).toBe(controller.signal));
    controller.abort();

    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
    expect(open).not.toHaveBeenCalled();
  });
});
