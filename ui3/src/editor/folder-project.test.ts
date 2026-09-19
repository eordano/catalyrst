import { webcrypto } from "node:crypto";
import { afterEach, expect, it, vi } from "vitest";
import { folderProject } from "./folder-project";

class MemoryFile {
  readonly kind = "file";
  duringWrite?: () => void;
  constructor(public name: string, public bytes = new ArrayBuffer(0)) {}
  async getFile() { const bytes = this.bytes.slice(0); return { size: bytes.byteLength, arrayBuffer: async () => bytes }; }
  async createWritable() {
    let next = this.bytes;
    return { write: async (value: string | ArrayBuffer) => { next = typeof value === "string" ? new TextEncoder().encode(value).buffer : value; this.duringWrite?.(); }, close: async () => { this.bytes = next; }, abort: async () => {} };
  }
}
class MemoryDirectory {
  readonly kind = "directory";
  readonly children = new Map<string, MemoryDirectory | MemoryFile>();
  constructor(public name: string) {}
  async *entries() { yield* this.children.entries(); }
  async getDirectoryHandle(name: string, options?: { create?: boolean }): Promise<MemoryDirectory> {
    let value = this.children.get(name);
    if (!value && options?.create) { value = new MemoryDirectory(name); this.children.set(name, value); }
    if (!(value instanceof MemoryDirectory)) throw new DOMException("missing", "NotFoundError");
    return value;
  }
  async getFileHandle(name: string, options?: { create?: boolean }): Promise<MemoryFile> {
    let value = this.children.get(name);
    if (!value && options?.create) { value = new MemoryFile(name); this.children.set(name, value); }
    if (!(value instanceof MemoryFile)) throw new DOMException("missing", "NotFoundError");
    return value;
  }
  async removeEntry(name: string) { this.children.delete(name); }
}
afterEach(() => vi.unstubAllGlobals());
it("uses the same project contract for disk source, custom items, assets, and revision-checked cleanup", async () => {
  vi.stubGlobal("crypto", webcrypto);
  const directory = new MemoryDirectory("scene");
  const project = folderProject(directory as unknown as FileSystemDirectoryHandle);
  await project.write("src/index.ts", "export function main() {}");
  await project.write("assets/custom-items/chair.composite", "{}");
  await project.assets!.write("assets/chair.png", new Uint8Array([1, 2]).buffer);
  expect(await project.list()).toEqual(["src/index.ts", "assets/custom-items/chair.composite", "assets/chair.png"]);
  expect(await project.read("assets/custom-items/chair.composite")).toBe("{}");
  const old = await project.assets!.read("assets/chair.png");
  await project.assets!.write("assets/chair.png", new Uint8Array([3]).buffer, old.revision);
  await expect(project.assets!.remove("assets/chair.png", old.revision)).rejects.toThrow(/changed after review/);
  await expect(project.assets!.write("assets/chair.png", new ArrayBuffer(0))).rejects.toThrow(/changed outside/);
  const fresh = await project.assets!.revision!("assets/chair.png");
  await project.assets!.remove("assets/chair.png", fresh);
  expect(await project.assets!.list()).toEqual([]);
  await expect(project.read("../outside.png")).rejects.toThrow(/Invalid asset path/);
});
it("keeps independent editor baselines and does not rebase writes during read-only inspection", async () => {
  vi.stubGlobal("crypto", webcrypto);
  const directory = new MemoryDirectory("scene");
  const project = folderProject(directory as unknown as FileSystemDirectoryHandle);
  await project.write("src/index.ts", "original");
  const first = project.createFileSession!(), second = project.createFileSession!();
  await first.read("src/index.ts");
  await second.read("src/index.ts");
  await second.write("src/index.ts", "external edit");
  expect(await first.readOnly!("src/index.ts")).toBe("external edit");
  await expect(first.write("src/index.ts", "stale edit")).rejects.toThrow(/changed outside/);
  expect(await project.readOnly!("src/index.ts")).toBe("external edit");
  await first.read("src/index.ts");
  await first.write("src/index.ts", "reopened edit");
  await first.write("src/index.ts", "next save");
  expect(await project.readOnly!("src/index.ts")).toBe("next save");
});
it("serializes same-baseline writes and create-only custom item collisions across sessions", async () => {
  vi.stubGlobal("crypto", webcrypto);
  const project = folderProject(new MemoryDirectory("scene") as unknown as FileSystemDirectoryHandle);
  await project.write("src/index.ts", "original");
  const first = project.createFileSession!(), second = project.createFileSession!();
  await Promise.all([first.read("src/index.ts"), second.read("src/index.ts")]);
  const saves = await Promise.allSettled([first.write("src/index.ts", "first"), second.write("src/index.ts", "second")]);
  expect(saves.map(result => result.status)).toEqual(["fulfilled", "rejected"]);
  expect(await project.readOnly!("src/index.ts")).toBe("first");
  const creates = await Promise.allSettled([first.write("assets/custom-items/chair.composite", "first"), second.write("assets/custom-items/chair.composite", "second")]);
  expect(creates.map(result => result.status)).toEqual(["fulfilled", "rejected"]);
  expect(await project.readOnly!("assets/custom-items/chair.composite")).toBe("first");
});
it("rejects unread writes/removal, deleted baselines, and changes during a staged write", async () => {
  vi.stubGlobal("crypto", webcrypto);
  const directory = new MemoryDirectory("scene");
  const project = folderProject(directory as unknown as FileSystemDirectoryHandle);
  await project.write("index.ts", "original");
  const session = project.createFileSession!();
  await expect(session.write("index.ts", "unread overwrite")).rejects.toThrow(/changed outside/);
  await expect(session.remove!("index.ts")).rejects.toThrow(/Read the source/);
  await session.read("index.ts");
  const file = await directory.getFileHandle("index.ts");
  file.duringWrite = () => { file.bytes = new TextEncoder().encode("outside during write").buffer; };
  await expect(session.write("index.ts", "staged edit")).rejects.toThrow(/changed outside/);
  expect(await session.readOnly!("index.ts")).toBe("outside during write");
  file.duringWrite = undefined;
  await expect(session.remove!("index.ts")).rejects.toThrow(/changed after review/);
  await session.read("index.ts");
  await session.remove!("index.ts");
  await expect(session.write("index.ts", "new file")).resolves.toBeUndefined();
  await session.read("index.ts");
  await directory.removeEntry("index.ts");
  await expect(session.write("index.ts", "resurrect stale file")).rejects.toThrow(/changed outside/);
});

it("prepares folder models and textures for preview while keeping saved paths relative", async () => {
  vi.stubGlobal("crypto", webcrypto);
  vi.stubGlobal("fetch", vi.fn(async () => Response.json({ content: { publicUrl: "https://preview.test/_project/content" } })));
  const cached = new Map<string, Response>();
  vi.stubGlobal("caches", { open: vi.fn(async () => ({ put: async (key: string, response: Response) => { cached.set(key, response); } })) });
  const directory = new MemoryDirectory("scene");
  const project = folderProject(directory as unknown as FileSystemDirectoryHandle);
  await project.assets!.write("assets/model.glb", new TextEncoder().encode("model").buffer);
  await project.assets!.write("assets/texture.png", new TextEncoder().encode("texture").buffer);
  await project.write("src/index.ts", "source");
  const content = await project.assets!.preparePreview!();
  expect(Object.keys(content)).toEqual(["assets/model.glb", "assets/texture.png"]);
  expect(content["assets/model.glb"]).toMatch(/^b64-[a-f0-9]{64}$/);
  const response = cached.get(`https://preview.test/_project/content/contents/${content["assets/model.glb"]}`)!;
  expect(await response.text()).toBe("model");
  expect(response.headers.get("content-type")).toBe("model/gltf-binary");
  expect(new TextDecoder().decode((await project.assets!.read("assets/model.glb")).content)).toBe("model");
});
