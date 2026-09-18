import { afterEach, describe, expect, test, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

import { BOOT_TIMEOUT_MS } from "../editor-config";
import { useProjectRealm } from "./useProjectRealm";
import { initialProjectRealmMachine, transitionProjectRealm } from "./project-realm-machine";

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

test("local template entry rebuilds a populated preview cache before mounting the engine", async () => {
  window.history.replaceState({}, "", "/creator-hub/scene-editor?source=local&project=memory-game");
  const caches = makeCaches();
  vi.stubGlobal("caches", caches);
  caches.store("ch-project-v1").set("stale-template", new Response("old deployment"));
  let finish!: () => void;
  const prepare = vi.fn(() => new Promise<void>(resolve => { finish = resolve; }));
  const hook = renderHook(() => useProjectRealm(VIEWPORT, prepare));
  await waitFor(() => expect(prepare).toHaveBeenCalledOnce());
  expect(hook.result.current).toBe("pending");
  await act(async () => { finish(); });
  await waitFor(() => expect(hook.result.current).toBe("ready"));
});

test("a failed local template refresh does not launch the stale cached scene", async () => {
  window.history.replaceState({}, "", "/creator-hub/scene-editor?source=local&project=memory-game");
  const caches = makeCaches();
  vi.stubGlobal("caches", caches);
  caches.store("ch-project-v1").set("stale-template", new Response("old deployment"));
  const hook = renderHook(() => useProjectRealm(VIEWPORT, async () => { throw new Error("template unavailable"); }));
  await waitFor(() => expect(hook.result.current).toBe("error"));
  expect(caches.store("ch-project-v1").has("stale-template")).toBe(true);
});

afterEach(() => {
  vi.useRealTimers();
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

test("realm machine rejects stale request and session completion", () => {
  const started = transitionProjectRealm(initialProjectRealmMachine(true), { type: "start", request: 2, session: 4 });
  expect(transitionProjectRealm(started, { type: "ready", request: 1, session: 4 })).toEqual(started);
  expect(transitionProjectRealm(started, { type: "error", request: 2, session: 3 })).toEqual(started);
  expect(transitionProjectRealm(started, { type: "ready", request: 2, session: 4 }).status).toBe("ready");
});

test("realm machine terminal completion cannot be overwritten", () => {
  const active = transitionProjectRealm(initialProjectRealmMachine(true), { type: "start", request: 1, session: 1 });
  const failed = transitionProjectRealm(active, { type: "error", request: 1, session: 1 });
  expect(transitionProjectRealm(failed, { type: "ready", request: 1, session: 1 })).toEqual(failed);
});

test("preparation rejection reaches error", async () => {
  const caches = makeCaches();
  vi.stubGlobal("caches", caches);
  const failed = renderHook(() => useProjectRealm(VIEWPORT, async () => { throw new Error("seed failed"); }));
  await waitFor(() => expect(failed.result.current).toBe("error"));
});

test("URL scope change rejects an older preparation completion", async () => {
  const caches = makeCaches();
  vi.stubGlobal("caches", caches);
  let releaseFirst!: () => void;
  let calls = 0;
  const signals: AbortSignal[] = [];
  const prepare = vi.fn((signal?: AbortSignal) => {
    if (signal) signals.push(signal);
    calls += 1;
    if (calls > 1) return (async () => {
      ((await caches.open("ch-project-v1")) as Cache).put("second", new Response("second"));
    })();
    return new Promise<void>(resolve => { releaseFirst = resolve; });
  });
  const hook = renderHook(({ url }) => useProjectRealm(url, prepare), { initialProps: { url: `${VIEWPORT}&project=one` } });
  await waitFor(() => expect(prepare).toHaveBeenCalledTimes(1));
  hook.rerender({ url: `${VIEWPORT}&project=two` });
  await new Promise(resolve => setTimeout(resolve, 20));
  expect(prepare).toHaveBeenCalledTimes(1);
  expect(signals[0]?.aborted).toBe(true);
  releaseFirst();
  await waitFor(() => expect(prepare).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(hook.result.current).toBe("ready"));
  hook.unmount();
});


test("a timed-out preparation cannot report ready later", async () => {
  vi.useFakeTimers();
  const cache = makeCaches();
  vi.stubGlobal("caches", cache);
  let complete!: () => void;
  let signal: AbortSignal | undefined;
  const started = vi.fn((nextSignal?: AbortSignal) => {
    signal = nextSignal;
    return new Promise<void>(resolve => { complete = resolve; });
  });
  const hook = renderHook(() => useProjectRealm(VIEWPORT, started));
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  expect(started).toHaveBeenCalledOnce();
  await act(async () => { await vi.advanceTimersByTimeAsync(BOOT_TIMEOUT_MS + 1); });
  expect(hook.result.current).toBe("error");
  expect(signal?.aborted).toBe(true);
  await act(async () => { complete(); });
  expect(hook.result.current).toBe("error");
});

test("drains stale preparation before replacement cache population", async () => {
  const caches = makeCaches();
  vi.stubGlobal("caches", caches);
  let releaseFirst!: () => void;
  const first = vi.fn(() => new Promise<void>((resolve) => {
    releaseFirst = async () => {
      ((await caches.open("ch-project-v1")) as Cache).put("first", new Response("first"));
      resolve();
    };
  }));
  const second = vi.fn(async () => {
    ((await caches.open("ch-project-v1")) as Cache).put("second", new Response("second"));
  });
  const hook = renderHook(
    ({ url, prepare }) => useProjectRealm(url, prepare),
    { initialProps: { url: `${VIEWPORT}&project=first`, prepare: first } },
  );
  await waitFor(() => expect(first).toHaveBeenCalledOnce());
  hook.rerender({ url: `${VIEWPORT}&project=second`, prepare: second });
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(second).not.toHaveBeenCalled();
  releaseFirst();
  await waitFor(() => expect(hook.result.current).toBe("ready"));
  expect(second).toHaveBeenCalledOnce();
  expect([...caches.store("ch-project-v1").keys()]).toEqual(["second"]);
});
