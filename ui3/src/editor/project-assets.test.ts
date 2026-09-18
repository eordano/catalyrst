import { afterEach, describe, expect, it, vi } from "vitest";
import { findUnreferencedAssets, persistCatalogAsset } from "./project-assets";
import { placeAssetOnBus } from "./project-cache";
import type { EditorBus } from "./editor-bus";
import type { ProjectAssets } from "./types";

const bytes = (text: string) => new TextEncoder().encode(text).buffer as ArrayBuffer;
function memoryAssets(initial: Record<string, string> = {}) {
  const files = new Map(Object.entries(initial).map(([path, text]) => [path, bytes(text)]));
  const store: ProjectAssets = {
    list: async () => [...files].map(([path, data]) => ({ path, size: data.byteLength })),
    read: async path => ({ content: files.get(path)!, revision: "v1" }),
    write: async (path, content) => { files.set(path, content); },
    remove: async path => { files.delete(path); },
  };
  return { files, store };
}
afterEach(() => vi.unstubAllGlobals());
describe("SDK project assets", () => {
  it("scans source references without advancing any editor's write revision", async () => {
    const { store } = memoryAssets({ "assets/used.png": "texture", "assets/unused.png": "texture" });
    const read = vi.fn(async () => "stale editable text");
    const readOnly = vi.fn(async () => '"assets/used.png"');
    const candidates = await findUnreferencedAssets({ assets: store, list: async () => ["src/ui.tsx"], read, readOnly }, "{}");
    expect(candidates.map(item => item.path)).toEqual(["assets/unused.png"]);
    expect(read).not.toHaveBeenCalled();
    expect(readOnly).toHaveBeenCalledWith("src/ui.tsx");
  });
  it("persists model plus textures under a shared project path before placement", async () => {
    const { files, store } = memoryAssets();
    const src = await persistCatalogAsset(store, { id: "tree", name: "Tree", glbFile: "model.glb", contents: { "model.glb": "modelHash", "textures/leaf.png": "textureHash" } }, async request => new Response(String(request)));
    expect(src).toBe("assets/imported/tree/model.glb");
    expect([...files.keys()]).toEqual(["assets/imported/tree/model.glb", "assets/imported/tree/textures/leaf.png"]);
  });
  it("persists a browser-only upload from the draft cache without requesting the server", async () => {
    const { files, store } = memoryAssets();
    const url = "https://catalyst.example.com/_project/content/contents/local-model.glb";
    const match = vi.fn(async (key: string) => key === url ? new Response("uploaded model") : undefined);
    const open = vi.fn(async () => ({ match }));
    vi.stubGlobal("caches", { open });
    const request = vi.fn(async () => new Response("missing", { status: 404 }));
    const path = await persistCatalogAsset(store, { id: "local:model.glb", name: "Model", glbUrl: url }, request);
    expect(new TextDecoder().decode(files.get(path))).toBe("uploaded model");
    expect(open).toHaveBeenCalledWith("ch-project-v1");
    expect(match).toHaveBeenCalledWith(url);
    expect(request).not.toHaveBeenCalled();
  });
  it("still downloads catalog assets when the draft cache is unavailable", async () => {
    vi.stubGlobal("caches", { open: vi.fn(async () => { throw new Error("storage disabled"); }) });
    const { files, store } = memoryAssets();
    const request = vi.fn(async () => new Response("downloaded model"));
    const path = await persistCatalogAsset(store, { id: "tree", name: "Tree", src: "/tree.glb" }, request);
    expect(new TextDecoder().decode(files.get(path))).toBe("downloaded model");
    expect(request).toHaveBeenCalledOnce();
  });
  it("does not create an entity when a catalog download or project save fails", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("unavailable", { status: 503 })));
    const addEntity = vi.fn();
    const bus = { current: { addEntity } as unknown as EditorBus };
    await expect(placeAssetOnBus(bus, { id: "tree", name: "Tree", src: "/tree.glb" }, null, memoryAssets().store)).rejects.toThrow(/download/);
    expect(addEntity).not.toHaveBeenCalled();
  });
  it("rejects traversal and preserves a file modified outside the editor", async () => {
    const { files, store } = memoryAssets({ "assets/imported/tree/model.glb": "user edited model" });
    const request = vi.fn(async () => new Response("catalog model"));
    await expect(persistCatalogAsset(store, { id: "tree", name: "Tree", glbFile: "model.glb", contents: { "../escape.glb": "hash" } }, request)).rejects.toThrow();
    expect(request).not.toHaveBeenCalled();
    await expect(persistCatalogAsset(store, { id: "tree", name: "Tree", src: "/model.glb" }, request)).rejects.toThrow(/different asset/);
    expect(new TextDecoder().decode(files.get("assets/imported/tree/model.glb"))).toBe("user edited model");
  });
  it("keeps transitive model textures and custom-item references when identifying candidates", async () => {
    const { store } = memoryAssets({ "assets/tree.gltf": '{"images":[{"uri":"leaf.png"}]}', "assets/leaf.png": "texture", "assets/chair.png": "texture", "assets/unused.png": "unused" });
    const project = { assets: store, list: async () => ["src/index.ts", "assets/custom-items/chair.composite"], read: async (path: string) => path.endsWith("index.ts") ? 'model: "assets/tree.gltf"' : 'texture: "assets/chair.png"' };
    expect(await findUnreferencedAssets(project, "{}")).toEqual([{ path: "assets/unused.png", size: 6 }]);
  });
  it("fails the review if any source or model dependencies cannot be inspected", async () => {
    const { store } = memoryAssets({ "assets/tree.glb": "invalid binary", "assets/leaf.png": "texture" });
    await expect(findUnreferencedAssets({ assets: store, list: async () => [], read: async () => "" }, '"assets/tree.glb"')).rejects.toThrow(/dependencies/);
  });
});

it("waits for entity acknowledgment and propagates a rejected placement instead of a success outcome", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response("model")));
  let reject!: (reason: Error) => void;
  const rpc = vi.fn(() => new Promise((_resolve, fail) => { reject = fail; }));
  const bus = { current: { rpc } as unknown as EditorBus };
  let completed = false;
  const outcome = placeAssetOnBus(bus, { id: "tree", name: "Tree", src: "/tree.glb" }, null, memoryAssets().store).then(value => { completed = true; return value; });
  await vi.waitFor(() => expect(rpc).toHaveBeenCalledWith("addEntity", ["Tree", 0, { GltfContainer: { src: "assets/imported/tree/model.glb" } }, null]));
  expect(completed).toBe(false);
  reject(new Error("The renderer rejected this entity"));
  await expect(outcome).rejects.toThrow(/renderer rejected/);
  expect(completed).toBe(false);
});

it("registers cached file mappings before asking the renderer to create an entity", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response("model")));
  const { store } = memoryAssets();
  store.preparePreview = vi.fn(async () => ({ "assets/imported/tree/model.glb": "b64-model" }));
  let acknowledge!: (count: number) => void;
  const rpc = vi.fn((method: string) => method === "registerContent" ? new Promise<number>(resolve => { acknowledge = resolve; }) : Promise.resolve("512"));
  const bus = { current: { rpc } as unknown as EditorBus };
  const outcome = placeAssetOnBus(bus, { id: "tree", name: "Tree", src: "/tree.glb" }, null, store);
  await vi.waitFor(() => expect(rpc).toHaveBeenCalledWith("registerContent", [{ "assets/imported/tree/model.glb": "b64-model" }], expect.any(Number)));
  expect(rpc).toHaveBeenCalledTimes(1);
  acknowledge(1);
  await expect(outcome).resolves.toMatchObject({ mirrored: true });
  expect(rpc).toHaveBeenLastCalledWith("addEntity", ["Tree", 0, { GltfContainer: { src: "assets/imported/tree/model.glb" } }, null]);
});

it("refuses placement when the renderer does not acknowledge all cached files", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response("model")));
  const { store } = memoryAssets();
  store.preparePreview = async () => ({ "assets/imported/tree/model.glb": "b64-model" });
  const rpc = vi.fn(async () => 0);
  const bus = { current: { rpc } as unknown as EditorBus };
  await expect(placeAssetOnBus(bus, { id: "tree", name: "Tree", src: "/tree.glb" }, null, store)).rejects.toThrow(/did not register/);
  expect(rpc).toHaveBeenCalledTimes(1);
});
