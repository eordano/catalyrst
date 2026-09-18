import type { DeWorkspaceCode, ProjectAssets } from "./types";
import { safeAssetPath } from "./project-assets";
import { PROJECT_CACHE, projectContentBase } from "./project-cache";

const SKIP = new Set(["node_modules", "bin", "dist", "build"]);
const ASSET = /\.(?:glb|gltf|bin|png|jpe?g|webp|gif|svg|ktx2?|mp3|wav|ogg|mp4|webm)$/i;
async function revision(content: ArrayBuffer): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", content);
  return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("");
}
export function folderProject(directory: FileSystemDirectoryHandle): NonNullable<DeWorkspaceCode["project"]> {
  const pending = new Map<string, Promise<void>>();
  const exclusive = async <T,>(path: string, action: () => Promise<T>): Promise<T> => {
    safeAssetPath(path);
    const previous = pending.get(path);
    let release!: () => void;
    const current = new Promise<void>(resolve => { release = resolve; });
    pending.set(path, current);
    await previous;
    try { return await action(); }
    finally { release(); if (pending.get(path) === current) pending.delete(path); }
  };
  const location = async (path: string, create = false) => {
    const parts = safeAssetPath(path).split("/");
    const name = parts.pop()!;
    let parent = directory;
    for (const part of parts) parent = await parent.getDirectoryHandle(part, { create });
    return { parent, name };
  };
  const list = async () => {
    const files: { path: string; size: number }[] = [];
    const walk = async (dir: FileSystemDirectoryHandle, prefix = "") => {
      for await (const [name, handle] of dir.entries()) {
        if (name.startsWith(".") || SKIP.has(name)) continue;
        const path = prefix + name;
        if (handle.kind === "directory") await walk(handle as FileSystemDirectoryHandle, path + "/");
        else files.push({ path, size: (await (handle as FileSystemFileHandle).getFile()).size });
      }
    };
    await walk(directory);
    return files;
  };
  const read = async (path: string) => {
    const { parent, name } = await location(path);
    return (await (await parent.getFileHandle(name)).getFile()).arrayBuffer();
  };
  const currentRevision = async (path: string): Promise<string | null> => {
    try { return await revision(await read(path)); }
    catch (error) { if (error instanceof DOMException && error.name === "NotFoundError") return null; throw error; }
  };
  const changed = (path: string) => new Error(`${path} changed outside the editor. Reopen it before saving your changes.`);
  const write = async (path: string, content: ArrayBuffer | string, expected: string | null) => exclusive(path, async () => {
    if (await currentRevision(path) !== expected) throw changed(path);
    const { parent, name } = await location(path, true);
    const handle = await parent.getFileHandle(name, { create: true });
    const baseline = expected ?? await revision(await (await handle.getFile()).arrayBuffer());
    const writer = await handle.createWritable();
    try {
      await writer.write(content);
      if (await currentRevision(path) !== baseline) throw changed(path);
      await writer.close();
    } catch (error) { await writer.abort().catch(() => {}); throw error; }
  });
  const remove = async (path: string, expected: string) => exclusive(path, async () => {
    if (await currentRevision(path) !== expected) throw new Error(`${path} changed after review. Read the file again before removing it.`);
    const { parent, name } = await location(path);
    await parent.removeEntry(name);
  });
  const assets: ProjectAssets = {
    preparePreview: async () => {
      const files = (await list()).filter(file => ASSET.test(file.path));
      if (!files.length) return {};
      const base = await projectContentBase();
      if (!base) throw new Error("The scene preview is unavailable. Reconnect before importing assets.");
      const cache = await caches.open(PROJECT_CACHE);
      const contents: Record<string, string> = {};
      const mime: Record<string, string> = { glb: "model/gltf-binary", gltf: "model/gltf+json", png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", webp: "image/webp", svg: "image/svg+xml" };
      for (const { path } of files) {
        const content = await read(path);
        const hash = `b64-${await revision(content)}`;
        await cache.put(`${base}/contents/${hash}`, new Response(content, {
          headers: { "content-type": mime[path.split(".").at(-1)!.toLowerCase()] ?? "application/octet-stream" },
        }));
        contents[path] = hash;
      }
      return contents;
    },
    list: async () => (await list()).filter(file => ASSET.test(file.path)),
    revision: async path => revision(await read(path)),
    read: async path => { const content = await read(path); return { content, revision: await revision(content) }; },
    write: (path, content, expected) => write(path, content, expected ?? null),
    remove,
  };
  const createFileSession = (): NonNullable<DeWorkspaceCode["project"]> => {
    const revisions = new Map<string, string>();
    return {
      id: directory.name,
      list: async () => (await list()).map(file => file.path),
      createFileSession,
      read: async path => {
        const content = await read(path);
        revisions.set(path, await revision(content));
        return new TextDecoder().decode(content);
      },
      readOnly: async path => new TextDecoder().decode(await read(path)),
      write: async (path, content) => {
        const bytes = new TextEncoder().encode(content).buffer;
        await write(path, bytes, revisions.get(path) ?? null);
        revisions.set(path, await revision(bytes));
      },
      remove: async path => {
        const expected = revisions.get(path);
        if (!expected) throw new Error("Read the source file before removing it.");
        await remove(path, expected);
        revisions.delete(path);
      },
      assets,
    };
  };
  return createFileSession();
}
