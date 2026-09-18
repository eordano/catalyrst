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

describe("handle-store", () => {
  it("stores a handle under a slug, reads back a structural clone, and keeps distinct slugs isolated", async () => {
    const store = createHandleStore(cloningBackend());
    const handle = fakeHandle("my-awesome-scene");
    await store.put("my-awesome-scene", handle);
    await store.put("b", fakeHandle("b"));

    const got = await store.get("my-awesome-scene");
    expect(got).toEqual(handle);
    expect(got).not.toBe(handle);
    expect(await store.get("b")).toMatchObject({ name: "b" });
  });

  it("returns null (never throws) for an absent or empty slug without touching the backend, and put(null) or clear removes an entry", async () => {
    const backend = cloningBackend();
    const store = createHandleStore(backend);
    await expect(store.get("nope")).resolves.toBeNull();
    expect(await store.get("")).toBeNull();
    await store.put("", fakeHandle("x"));
    expect(backend.size()).toBe(0);

    await store.put("scene", fakeHandle("scene"));
    expect(await store.get("scene")).not.toBeNull();
    await store.put("scene", null);
    expect(await store.get("scene")).toBeNull();

    await store.put("other", fakeHandle("other"));
    await store.clear("other");
    expect(await store.get("other")).toBeNull();
    expect(backend.size()).toBe(0);
  });

  it("putMeta registers a project that list() returns newest first, merges later saves, and clear removes BOTH the handle and the meta record", async () => {
    const store = createHandleStore(cloningBackend());
    await store.putMeta("alpha", { title: "Alpha", base: "0,0", updatedAt: 100 });
    await store.putMeta("beta", { title: "Beta", updatedAt: 200 });
    await store.putMeta("alpha", { composite: '{"components":[]}', updatedAt: 300 });

    const list = await store.list();
    expect(list?.map((m) => m.slug)).toEqual(["alpha", "beta"]);
    expect(await store.keys()).toEqual(expect.arrayContaining(["alpha", "beta"]));
    expect(await store.getMeta("alpha")).toMatchObject({
      title: "Alpha",
      base: "0,0",
      composite: '{"components":[]}',
      updatedAt: 300,
    });

    await store.put("alpha", fakeHandle("alpha"));
    await store.clear("alpha");
    expect(await store.get("alpha")).toBeNull();
    expect(await store.getMeta("alpha")).toBeNull();
    expect((await store.list())?.map((m) => m.slug)).toEqual(["beta"]);
  });

  it("slugifyProjectTitle produces stable url-safe slugs shared by save + reopen", () => {
    expect(slugifyProjectTitle("My Awesome Scene")).toBe("my-awesome-scene");
    expect(slugifyProjectTitle("  Trim & Symbols!! ")).toBe("trim-symbols");
    expect(slugifyProjectTitle("")).toBe("scene");
  });

  it("the default store no-ops without indexedDB (node): put resolves, get is null", async () => {
    expect(typeof indexedDB).toBe("undefined");
    await expect(handleStore.put("x", fakeHandle("x"))).resolves.toBeUndefined();
    await expect(handleStore.get("x")).resolves.toBeNull();
    await expect(handleStore.clear("x")).resolves.toBeUndefined();
  });
});

it("does not commit metadata after cancellation while reading the previous revision", async () => {
  const backend = cloningBackend();
  const controller = new AbortController();
  backend.readMeta = async () => { controller.abort(); return undefined; };
  const store = createHandleStore(backend);
  await expect(store.putMeta("scene", { title: "Retired" }, controller.signal)).rejects.toMatchObject({ name: "AbortError" });
  expect(await backend.listMeta()).toEqual([]);
});
