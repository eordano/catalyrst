import { afterEach, describe, expect, test, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";

import { useProjectRealm } from "./useProjectRealm";

type Store = Map<string, Response>;

function makeCaches(): { open: (n: string) => Promise<unknown>; has: (n: string) => Promise<boolean>; store: (n: string) => Store } {
  const stores = new Map<string, Store>();
  const store = (name: string): Store => {
    let m = stores.get(name);
    if (!m) {
      m = new Map();
      stores.set(name, m);
    }
    return m;
  };
  const wrap = (name: string) => {
    const m = store(name);
    return {
      keys: async () => [...m.keys()].map((url) => ({ url })),
      delete: async (req: { url: string } | string) => m.delete(typeof req === "string" ? req : req.url),
      put: async (url: string, resp: Response) => {
        m.set(url, resp);
      },
      match: async (url: string) => m.get(url),
    };
  };
  return {
    open: async (name: string) => wrap(name),
    has: async (name: string) => stores.has(name),
    store,
  };
}

const VIEWPORT = "/_play/?realm=%2F_project";

afterEach(() => {
  vi.unstubAllGlobals();
  window.history.replaceState({}, "", "/");
});

describe("useProjectRealm draft reopen seed", () => {
  test("retries populate until the realm cache is seeded before becoming ready", async () => {
    window.history.replaceState({}, "", "/creator-hub/scene-editor?draft=abc");
    const caches = makeCaches();
    vi.stubGlobal("caches", caches);
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false }) as Response));

    let calls = 0;
    const prepareRealm = vi.fn(async () => {
      calls += 1;
      if (calls >= 3) {
        (await caches.open("ch-project-v1") as { put: (u: string, r: Response) => Promise<void> }).put(
          "https://x/_project/content/contents/deadbeef",
          new Response("{}"),
        );
      }
    });

    const { result } = renderHook(() => useProjectRealm(VIEWPORT, prepareRealm));

    await waitFor(() => expect(result.current).toBe("ready"), { timeout: 5000 });

    expect(prepareRealm.mock.calls.length).toBeGreaterThanOrEqual(3);
    expect(caches.store("ch-project-v1").size).toBeGreaterThan(0);
  });

  test("seeds on the first successful populate without extra retries", async () => {
    window.history.replaceState({}, "", "/creator-hub/scene-editor?draft=abc");
    const caches = makeCaches();
    vi.stubGlobal("caches", caches);
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false }) as Response));

    const prepareRealm = vi.fn(async () => {
      (await caches.open("ch-project-v1") as { put: (u: string, r: Response) => Promise<void> }).put(
        "https://x/_project/content/contents/deadbeef",
        new Response("{}"),
      );
    });

    const { result } = renderHook(() => useProjectRealm(VIEWPORT, prepareRealm));

    await waitFor(() => expect(result.current).toBe("ready"), { timeout: 5000 });

    expect(prepareRealm.mock.calls.length).toBe(1);
  });
});
