import { afterEach, describe, expect, test, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";

import { useProjectRealm } from "./useProjectRealm";

type Store = Map<string, Response>;
type Cache = { put: (u: string, r: Response) => Promise<void> };

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
  test("retries populate until the realm cache is seeded, and stops after the first populate that seeds it", async () => {
    window.history.replaceState({}, "", "/creator-hub/scene-editor?draft=abc");
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false }) as Response));

    const slow = makeCaches();
    vi.stubGlobal("caches", slow);
    let calls = 0;
    const eventually = vi.fn(async () => {
      calls += 1;
      if (calls >= 3) {
        ((await slow.open("ch-project-v1")) as Cache).put(
          "https://x/_project/content/contents/deadbeef",
          new Response("{}"),
        );
      }
    });
    const first = renderHook(() => useProjectRealm(VIEWPORT, eventually));
    await waitFor(() => expect(first.result.current).toBe("ready"), { timeout: 5000 });
    expect(eventually.mock.calls.length).toBeGreaterThanOrEqual(3);
    expect(slow.store("ch-project-v1").size).toBeGreaterThan(0);
    first.unmount();

    const quick = makeCaches();
    vi.stubGlobal("caches", quick);
    const immediate = vi.fn(async () => {
      ((await quick.open("ch-project-v1")) as Cache).put(
        "https://x/_project/content/contents/deadbeef",
        new Response("{}"),
      );
    });
    const second = renderHook(() => useProjectRealm(VIEWPORT, immediate));
    await waitFor(() => expect(second.result.current).toBe("ready"), { timeout: 5000 });
    expect(immediate.mock.calls.length).toBe(1);
  });
});
