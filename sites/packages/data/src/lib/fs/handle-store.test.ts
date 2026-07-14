import { describe, expect, it } from "vitest";

import {
  createHandleStore,
  handleStore,
  slugifyProjectTitle,
  type HandleBackend,
  type ProjectMeta,
} from "./handle-store";

const fakeHandle = (name: string) =>
  ({ kind: "directory", name }) as unknown as FileSystemDirectoryHandle;

function cloningBackend(): HandleBackend & { size: () => number } {
  const map = new Map<string, unknown>();
  const meta = new Map<string, unknown>();
  return {
    read: async (key) => (map.has(key) ? structuredClone(map.get(key)) : undefined),
    write: async (key, value) => {
      map.set(key, structuredClone(value));
    },
    remove: async (key) => {
      map.delete(key);
    },
    readMeta: async (key) => (meta.has(key) ? structuredClone(meta.get(key)) : undefined),
    writeMeta: async (key, value) => {
      meta.set(key, structuredClone(value));
    },
    removeMeta: async (key) => {
      meta.delete(key);
    },
    listMeta: async () =>
      Array.from(meta.values()).map((m) => structuredClone(m)) as ProjectMeta[],
    size: () => map.size,
  };
}

describe("handle-store \u{2014} put/get round-trip", () => {
  it("stores a handle under a slug and reads it back", async () => {
    const store = createHandleStore(cloningBackend());
    const handle = fakeHandle("my-awesome-scene");

    await store.put("my-awesome-scene", handle);
    const got = await store.get("my-awesome-scene");

    expect(got).toEqual(handle);
    expect(got).not.toBe(handle);
  });

  it("keeps distinct slugs isolated", async () => {
    const store = createHandleStore(cloningBackend());
    await store.put("a", fakeHandle("a"));
    await store.put("b", fakeHandle("b"));

    expect(await store.get("a")).toMatchObject({ name: "a" });
    expect(await store.get("b")).toMatchObject({ name: "b" });
  });
});

describe("handle-store \u{2014} missing key", () => {
  it("returns null (never throws) for an absent slug", async () => {
    const store = createHandleStore(cloningBackend());
    await expect(store.get("nope")).resolves.toBeNull();
  });

  it("returns null for an empty slug without touching the backend", async () => {
    const backend = cloningBackend();
    const store = createHandleStore(backend);
    expect(await store.get("")).toBeNull();
    await store.put("", fakeHandle("x"));
    expect(backend.size()).toBe(0);
  });
});

describe("handle-store \u{2014} clearing an entry", () => {
  it("put(slug, null) removes the entry so get returns null", async () => {
    const store = createHandleStore(cloningBackend());
    await store.put("scene", fakeHandle("scene"));
    expect(await store.get("scene")).not.toBeNull();

    await store.put("scene", null);
    expect(await store.get("scene")).toBeNull();
  });

  it("clear(slug) removes the entry", async () => {
    const store = createHandleStore(cloningBackend());
    await store.put("scene", fakeHandle("scene"));
    await store.clear("scene");
    expect(await store.get("scene")).toBeNull();
  });
});

describe("handle-store \u{2014} local project library (meta)", () => {
  it("putMeta registers a project that list() returns, newest first", async () => {
    const store = createHandleStore(cloningBackend());
    await store.putMeta("alpha", { title: "Alpha", updatedAt: 100 });
    await store.putMeta("beta", { title: "Beta", updatedAt: 200 });

    const list = await store.list();
    expect(list).not.toBeNull();
    if (!list) return;
    expect(list.map((m) => m.slug)).toEqual(["beta", "alpha"]);
    expect(list[0]).toMatchObject({ slug: "beta", title: "Beta" });
    expect(await store.keys()).toEqual(expect.arrayContaining(["alpha", "beta"]));
  });

  it("putMeta merges \u{2014} a later save keeps the title and updates the composite", async () => {
    const store = createHandleStore(cloningBackend());
    await store.putMeta("s", { title: "My Scene", base: "0,0", updatedAt: 1 });
    await store.putMeta("s", { composite: '{"components":[]}', updatedAt: 2 });

    const meta = await store.getMeta("s");
    expect(meta).toMatchObject({
      title: "My Scene",
      base: "0,0",
      composite: '{"components":[]}',
      updatedAt: 2,
    });
  });

  it("clear removes BOTH the handle and the meta record", async () => {
    const store = createHandleStore(cloningBackend());
    await store.put("s", fakeHandle("s"));
    await store.putMeta("s", { title: "S" });

    await store.clear("s");
    expect(await store.get("s")).toBeNull();
    expect(await store.getMeta("s")).toBeNull();
    expect(await store.list()).toEqual([]);
  });
});

describe("handle-store \u{2014} slugifyProjectTitle", () => {
  it("produces stable url-safe slugs shared by save + reopen", () => {
    expect(slugifyProjectTitle("My Awesome Scene")).toBe("my-awesome-scene");
    expect(slugifyProjectTitle("  Trim & Symbols!! ")).toBe("trim-symbols");
    expect(slugifyProjectTitle("")).toBe("scene");
  });
});

describe("handle-store \u{2014} SSR / no-IndexedDB safety", () => {
  it("the default store no-ops without indexedDB (node): put resolves, get is null", async () => {
    expect(typeof indexedDB).toBe("undefined");
    await expect(handleStore.put("x", fakeHandle("x"))).resolves.toBeUndefined();
    await expect(handleStore.get("x")).resolves.toBeNull();
    await expect(handleStore.clear("x")).resolves.toBeUndefined();
  });
});
