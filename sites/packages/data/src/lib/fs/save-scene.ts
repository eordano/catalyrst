import {
  addEntity,
  buildCompositeFromHierarchy,
  deleteEntity,
  listEntities,
  parseComposite,
  renameEntity,
  serializeSceneComposite,
  setComponentValue,
  type SceneComposite,
} from "../catalyst/creator-hub/scene-composite";
import { catalystBase } from "../catalyst/client";
import { saveTextFile, type SaveResult as DiskSaveResult } from "./disk";
import { crc32 } from "./zip";

export const COMPOSITE_FILENAME = "main.composite";
export const SCENE_JSON_FILENAME = "scene.json";

const COMPOSITE_PATH_RE = /(^|\/)main\.composite$/i;
const SCENE_JSON_PATH_RE = /(^|\/)scene\.json$/i;

export type SceneHierarchyNode = {
  entity: number;
  name: string;
  parent: number;
};

export type SaveSceneEdit = {
  placed?: { entity: number; assetId?: string; assetName: string; parent?: number };
  selected?: { entity: number; name: string };
  modifiedName?: string;
  component?: string;
  deleted?: boolean;
};

export function composeEditedScene(
  hierarchy: ReadonlyArray<SceneHierarchyNode>,
  edit: SaveSceneEdit = {},
): SceneComposite {
  let c = buildCompositeFromHierarchy(hierarchy);

  if (edit.placed) {
    c = addEntity(c, {
      id: edit.placed.entity,
      name: edit.placed.assetName,
      parent: edit.placed.parent,
    }).composite;
  }

  if (edit.selected && edit.modifiedName) {
    c = renameEntity(c, edit.selected.entity, edit.modifiedName);
  }

  if (edit.component) {
    const target = edit.placed?.entity ?? edit.selected?.entity;
    if (typeof target === "number") {
      c = setComponentValue(c, target, edit.component, {});
    }
  }

  if (edit.deleted && edit.selected) {
    c = deleteEntity(c, edit.selected.entity);
  }

  return c;
}

export type SaveSceneResult = {
  written: boolean;
  via: "fsa-handle" | "download" | "canceled";
  text: string;
  entities: number;
  filename: string;
  contentWarnings?: string[];
};

export type SaveSceneOptions = {
  handle?: FileSystemFileHandle | null;
  signal?: AbortSignal;
  writer?: (
    filename: string,
    text: string,
    handle?: FileSystemFileHandle | null,
  ) => Promise<DiskSaveResult>;
  project?: {
    slug?: string;
    title?: string;
    base?: string;
    template?: string;
    assets?: Record<string, string>;
  };
};

async function persistCompositeText(
  text: string,
  entities: number,
  opts: SaveSceneOptions = {},
): Promise<SaveSceneResult> {
  const deployed = await persistDeployedProject(text, entities, opts);
  if (deployed) return deployed;

  let handle = opts.handle ?? null;
  if (!handle && !opts.writer && typeof window !== "undefined") {
    try {
      const { readCompositeHandle } = await import("./local-scene");
      handle = await readCompositeHandle();
    } catch {
      handle = null;
    }
  }

  // Wizard-created projects have no stashed composite handle, but the create
  // wizard persisted their project-folder handle -- write there instead of
  // falling through to the save-file picker.
  if (!handle && !opts.writer && typeof window !== "undefined") {
    try {
      const { handleStore, slugifyProjectTitle, ensureHandlePermission } = await import(
        "./handle-store"
      );
      const urlSlug = new URLSearchParams(window.location.search).get("project")?.trim();
      const slug =
        opts.project?.slug?.trim() ||
        urlSlug ||
        slugifyProjectTitle(opts.project?.title?.trim() || "");
      const dir = slug ? await handleStore.get(slug).catch(() => null) : null;
      if (
        dir &&
        (await ensureHandlePermission(dir, "readwrite")) &&
        (await writeBytesAtPath(dir, COMPOSITE_FILENAME, text))
      ) {
        await registerLocalProject(text, opts.project);
        return {
          written: true,
          via: "fsa-handle",
          text,
          entities,
          filename: COMPOSITE_FILENAME,
        };
      }
    } catch {
    }
  }

  const write =
    opts.writer ??
    ((name, body, h) => saveTextFile(name, body, h, { onAbort: "cancel" }));
  const result = await write(COMPOSITE_FILENAME, text, handle);

  const out: SaveSceneResult = {
    written: result !== "canceled",
    via:
      result === "written"
        ? "fsa-handle"
        : result === "downloaded"
          ? "download"
          : "canceled",
    text,
    entities,
    filename: COMPOSITE_FILENAME,
  };

  if (out.written) await registerLocalProject(text, opts.project);
  return out;
}

async function registerLocalProject(
  compositeText: string,
  project?: SaveSceneOptions["project"],
): Promise<void> {
  if (typeof window === "undefined" || project === undefined) return;
  try {
    const { handleStore, slugifyProjectTitle } = await import("./handle-store");
    const urlSlug = new URLSearchParams(window.location.search).get("project")?.trim();
    const title = project.title?.trim() || "Untitled scene";
    const slug = project.slug?.trim() || urlSlug || slugifyProjectTitle(title);
    await handleStore.putMeta(slug, {
      title,
      base: project.base,
      composite: compositeText,
      ...(project.template ? { template: project.template } : {}),
      ...(project.assets && Object.keys(project.assets).length > 0
        ? { assets: project.assets }
        : {}),
      updatedAt: Date.now(),
    });
  } catch {
  }
}

export async function saveSceneComposite(
  hierarchy: ReadonlyArray<SceneHierarchyNode>,
  edit: SaveSceneEdit = {},
  opts: SaveSceneOptions = {},
): Promise<SaveSceneResult> {
  const composite = composeEditedScene(hierarchy, edit);
  const text = serializeSceneComposite(composite);
  const entities = listEntities(composite).length;
  return persistCompositeText(text, entities, opts);
}

export type SaveSceneFromEngineResult = SaveSceneResult & {
  source: "engine";
};

export const ENGINE_GAP_SAVE_ERROR =
  "The 3D engine didn't answer with the scene contents, so saving now would " +
  "overwrite your project with a stripped copy (models removed). Wait for the " +
  "viewport to finish loading \u{2014} or reload the editor \u{2014} and save again.";

export function normalizeEngineComposite(out: unknown): string | null {
  if (out == null) return null;
  const text = typeof out === "string" ? out : JSON.stringify(out);
  if (!text.trim()) return null;
  try {
    parseComposite(JSON.parse(text));
    return text;
  } catch {
    return null;
  }
}

async function readEngineComposite(
  timeoutMs: number,
  inject?: (() => Promise<unknown>) | null,
): Promise<string | null> {
  if (inject) {
    try {
      return normalizeEngineComposite(await inject());
    } catch {
      return null;
    }
  }
  if (typeof window === "undefined" || typeof BroadcastChannel === "undefined") {
    return null;
  }
  try {
    const mod = (await import("@ui/editor/editor-bus")) as {
      createEditorBus: () => {
        exportComposite: (t?: number) => Promise<unknown>;
        close?: () => void;
        dispose?: () => void;
      };
    };
    const bus = mod.createEditorBus();
    try {
      return normalizeEngineComposite(await bus.exportComposite(timeoutMs));
    } finally {
      bus.close?.();
      bus.dispose?.();
    }
  } catch {
    return null;
  }
}


const builderAssetCache = new Map<
  string,
  { token: string; ext: string; buf: ArrayBuffer }
>();
const persistedContentNames = new Set<string>();

const BUILDER_ITEMS_RE = /\/builder-items\//;
export const SEEDED_ASSET_SRC_RE = /^assets\/imported\/[^/]+\/([A-Za-z0-9]{40,})\.(glb|gltf)$/;
const SEEDED_HINT_RE = /assets\/imported\//;

async function subtleToken(buf: ArrayBuffer): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", buf);
  const hex = Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
  return `b64-${hex}`;
}

function gltfExt(buf: ArrayBuffer): string {
  try {
    const head = new Uint8Array(buf.slice(0, 4));
    const magic = String.fromCharCode(head[0], head[1], head[2], head[3]);
    return magic === "glTF" ? "glb" : "gltf";
  } catch {
    return "glb";
  }
}

async function resolveProjectDir(): Promise<FileSystemDirectoryHandle | null> {
  if (typeof window === "undefined") return null;
  try {
    const slug = new URLSearchParams(window.location.search).get("project")?.trim();
    if (!slug) return null;
    const { handleStore, ensureHandlePermission } = await import("./handle-store");
    const handle = await handleStore.get(slug);
    if (!handle) return null;
    return (await ensureHandlePermission(handle, "readwrite")) ? handle : null;
  } catch {
    return null;
  }
}

async function writeContentFile(
  dir: FileSystemDirectoryHandle,
  name: string,
  buf: ArrayBuffer,
): Promise<boolean> {
  try {
    const contents = await (
      dir as unknown as {
        getDirectoryHandle: (
          n: string,
          o?: { create?: boolean },
        ) => Promise<FileSystemDirectoryHandle>;
      }
    ).getDirectoryHandle("contents", { create: true });
    const fh = await (
      contents as unknown as {
        getFileHandle: (
          n: string,
          o?: { create?: boolean },
        ) => Promise<FileSystemFileHandle>;
      }
    ).getFileHandle(name, { create: true });
    const createWritable = (
      fh as unknown as {
        createWritable?: () => Promise<{
          write: (d: BufferSource) => Promise<void>;
          close: () => Promise<void>;
        }>;
      }
    ).createWritable;
    if (typeof createWritable !== "function") return false;
    const writable = await createWritable.call(fh);
    await writable.write(buf);
    await writable.close();
    return true;
  } catch {
    return false;
  }
}

async function persistBuilderAssets(
  text: string,
  dirOverride?: FileSystemDirectoryHandle | null,
): Promise<string> {
  if (typeof window === "undefined" || typeof fetch === "undefined") return text;
  if (typeof crypto === "undefined" || !crypto.subtle) return text;
  if (!BUILDER_ITEMS_RE.test(text) && !SEEDED_HINT_RE.test(text)) return text;

  let composite: unknown;
  try {
    composite = JSON.parse(text);
  } catch {
    return text;
  }

  const holders: { obj: Record<string, unknown>; src: string }[] = [];
  const seededSrcs = new Set<string>();
  try {
    const components = (composite as { components?: unknown }).components;
    if (!Array.isArray(components)) return text;
    for (const block of components) {
      if (!block || typeof block !== "object") continue;
      const name = (block as { name?: unknown }).name;
      if (typeof name !== "string" || !/GltfContainer$/.test(name)) continue;
      const data = (block as { data?: unknown }).data;
      if (!data || typeof data !== "object") continue;
      for (const entry of Object.values(data as Record<string, unknown>)) {
        const val =
          entry && typeof entry === "object" && "json" in (entry as object)
            ? (entry as { json?: unknown }).json
            : entry;
        if (!val || typeof val !== "object") continue;
        const src = (val as { src?: unknown }).src;
        if (typeof src !== "string") continue;
        if (BUILDER_ITEMS_RE.test(src)) {
          holders.push({ obj: val as Record<string, unknown>, src });
        } else if (SEEDED_ASSET_SRC_RE.test(src)) {
          seededSrcs.add(src);
        }
      }
    }
  } catch {
    return text;
  }
  if (holders.length === 0 && seededSrcs.size === 0) return text;

  const dir = dirOverride ?? (await resolveProjectDir());
  if (!dir) return text;

  for (const src of seededSrcs) {
    try {
      if (persistedContentNames.has(src)) continue;
      if (await pathHasFile(dir, src)) {
        persistedContentNames.add(src);
        continue;
      }
      const cid = SEEDED_ASSET_SRC_RE.exec(src)?.[1];
      if (!cid) continue;
      let cached = builderAssetCache.get(src);
      if (!cached) {
        const res = await fetch(`/builder-items/${cid}`, { credentials: "omit" });
        if (!res.ok) continue;
        const buf = await res.arrayBuffer();
        cached = { token: cid, ext: gltfExt(buf), buf };
        builderAssetCache.set(src, cached);
      }
      if (await writeBytesAtPath(dir, src, new Uint8Array(cached.buf))) {
        persistedContentNames.add(src);
      }
    } catch {
    }
  }
  if (holders.length === 0) return text;

  const rewrite = new Map<string, string>();
  const srcs = new Set(holders.map((h) => h.src));
  for (const src of srcs) {
    try {
      let cached = builderAssetCache.get(src);
      if (!cached) {
        const res = await fetch(src, { credentials: "omit" });
        if (!res.ok) continue;
        const buf = await res.arrayBuffer();
        cached = { token: await subtleToken(buf), ext: gltfExt(buf), buf };
        builderAssetCache.set(src, cached);
      }
      const fileName = `${cached.token}.${cached.ext}`;
      if (!persistedContentNames.has(fileName)) {
        if (!(await writeContentFile(dir, fileName, cached.buf))) continue;
        persistedContentNames.add(fileName);
      }
      rewrite.set(src, `contents/${fileName}`);
    } catch {
    }
  }
  if (rewrite.size === 0) return text;

  for (const h of holders) {
    const rel = rewrite.get(h.src);
    if (rel) h.obj.src = rel;
  }
  try {
    return JSON.stringify(composite);
  } catch {
    return text;
  }
}

export async function saveSceneFromEngine(
  _fallbackHierarchy: ReadonlyArray<SceneHierarchyNode>,
  _fallbackEdit: SaveSceneEdit = {},
  opts: SaveSceneOptions & {
    timeoutMs?: number;
    exportComposite?: (() => Promise<unknown>) | null;
  } = {},
): Promise<SaveSceneFromEngineResult> {
  let text = await readEngineComposite(opts.timeoutMs ?? 12000, opts.exportComposite);
  if (text === null) {
    throw new Error(ENGINE_GAP_SAVE_ERROR);
  }

  try {
    text = await persistBuilderAssets(text);
  } catch {
  }

  let entities = 0;
  try {
    entities = listEntities(parseComposite(JSON.parse(text))).length;
  } catch {
    entities = 0;
  }

  const result = await persistCompositeText(text, entities, opts);
  return { ...result, source: "engine" };
}

export type DeployedEntityFiles = {
  pointer: string;
  title: string;
  base: string;
  metadataText: string;
  contents: { file: string; hash: string }[];
};

function entityFilesFrom(raw: unknown, fallbackPointer: string): DeployedEntityFiles | null {
  if (!raw || typeof raw !== "object") return null;
  const e = raw as {
    type?: unknown;
    pointers?: unknown;
    content?: unknown;
    metadata?: unknown;
  };
  if (e.type !== "scene") return null;
  const pointers = Array.isArray(e.pointers)
    ? e.pointers.filter((p): p is string => typeof p === "string")
    : [];
  const contents: { file: string; hash: string }[] = [];
  if (Array.isArray(e.content)) {
    for (const c of e.content) {
      if (!c || typeof c !== "object") continue;
      const file = (c as { file?: unknown; key?: unknown }).file ?? (c as { key?: unknown }).key;
      const hash = (c as { hash?: unknown }).hash;
      if (typeof file === "string" && file && typeof hash === "string" && hash) {
        contents.push({ file, hash });
      }
    }
  }
  const md =
    e.metadata && typeof e.metadata === "object"
      ? (e.metadata as { display?: { title?: unknown }; scene?: { base?: unknown } })
      : null;
  const title =
    md && md.display && typeof md.display.title === "string" && md.display.title.trim()
      ? md.display.title.trim()
      : "Imported scene";
  const base =
    md && md.scene && typeof md.scene.base === "string" && md.scene.base
      ? md.scene.base
      : pointers[0] ?? fallbackPointer;
  const metadataText = JSON.stringify(e.metadata ?? { scene: { base, parcels: pointers } }, null, 2);
  return { pointer: pointers[0] ?? fallbackPointer, title, base, metadataText, contents };
}

export async function fetchDeployedEntityByPointer(
  pointer: string,
): Promise<DeployedEntityFiles | null> {
  try {
    const res = await fetch(`${catalystBase()}/content/entities/active`, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({ pointers: [pointer] }),
    });
    if (!res.ok) return null;
    const raw = (await res.json()) as unknown;
    if (!Array.isArray(raw) || raw.length === 0) return null;
    return entityFilesFrom(raw[0], pointer);
  } catch {
    return null;
  }
}

export async function fetchDeployedEntityById(
  entityId: string,
): Promise<DeployedEntityFiles | null> {
  try {
    const res = await fetch(`${catalystBase()}/content/contents/${entityId}`, {
      headers: { accept: "application/json" },
    });
    if (!res.ok) return null;
    return entityFilesFrom((await res.json()) as unknown, "");
  } catch {
    return null;
  }
}

type BinZipEntry = { path: string; data: Uint8Array };

const zipEnc = new TextEncoder();

const zu16 = (n: number) => new Uint8Array([n & 0xff, (n >>> 8) & 0xff]);
const zu32 = (n: number) =>
  new Uint8Array([n & 0xff, (n >>> 8) & 0xff, (n >>> 16) & 0xff, (n >>> 24) & 0xff]);

function zipConcat(parts: Uint8Array[]): Uint8Array {
  let total = 0;
  for (const p of parts) total += p.length;
  const out = new Uint8Array(total);
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

export function makeBinaryZip(entries: BinZipEntry[]): Uint8Array {
  const local: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;
  const DATE = 0x0021;

  for (const e of entries) {
    const name = zipEnc.encode(e.path);
    const crc = crc32(e.data);
    const size = e.data.length;

    const lfh = zipConcat([
      zu32(0x04034b50), zu16(20), zu16(0), zu16(0),
      zu16(0), zu16(DATE),
      zu32(crc), zu32(size), zu32(size),
      zu16(name.length), zu16(0),
      name,
    ]);
    local.push(lfh, e.data);

    central.push(
      zipConcat([
        zu32(0x02014b50), zu16(20), zu16(20), zu16(0), zu16(0),
        zu16(0), zu16(DATE),
        zu32(crc), zu32(size), zu32(size),
        zu16(name.length), zu16(0), zu16(0),
        zu16(0), zu16(0), zu32(0),
        zu32(offset),
        name,
      ]),
    );
    offset += lfh.length + e.data.length;
  }

  const centralBytes = zipConcat(central);
  const eocd = zipConcat([
    zu32(0x06054b50), zu16(0), zu16(0),
    zu16(entries.length), zu16(entries.length),
    zu32(centralBytes.length), zu32(offset),
    zu16(0),
  ]);
  return zipConcat([...local, centralBytes, eocd]);
}

function downloadBlob(name: string, bytes: Uint8Array, type: string): void {
  if (typeof document === "undefined") return;
  const buf = bytes.slice().buffer as ArrayBuffer;
  const url = URL.createObjectURL(new Blob([buf], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.rel = "noopener";
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

const CONTENT_FETCH_CONCURRENCY = 4;

async function fetchContentBytes(
  contents: { file: string; hash: string }[],
): Promise<{ entries: BinZipEntry[]; failed: string[] }> {
  const base = catalystBase();
  const entries: BinZipEntry[] = [];
  const failed: string[] = [];
  for (let i = 0; i < contents.length; i += CONTENT_FETCH_CONCURRENCY) {
    const batch = contents.slice(i, i + CONTENT_FETCH_CONCURRENCY);
    await Promise.all(
      batch.map(async (c) => {
        try {
          const res = await fetch(`${base}/content/contents/${c.hash}`, {
            credentials: "omit",
          });
          if (!res.ok) {
            failed.push(c.file);
            return;
          }
          entries.push({ path: c.file, data: new Uint8Array(await res.arrayBuffer()) });
        } catch {
          failed.push(c.file);
        }
      }),
    );
  }
  return { entries, failed };
}

export async function downloadDeployedEntityZip(
  entityId: string,
  title?: string,
): Promise<boolean> {
  const entity = await fetchDeployedEntityById(entityId);
  if (!entity) return false;
  const { slugifyProjectTitle } = await import("./handle-store");
  const { entries } = await fetchContentBytes(entity.contents);
  if (!entity.contents.some((c) => SCENE_JSON_PATH_RE.test(c.file))) {
    entries.unshift({ path: SCENE_JSON_FILENAME, data: zipEnc.encode(entity.metadataText) });
  }
  if (entries.length === 0) return false;
  const slug = slugifyProjectTitle(title?.trim() || entity.title);
  downloadBlob(`${slug}.zip`, makeBinaryZip(entries), "application/zip");
  return true;
}

type DirLike = FileSystemDirectoryHandle & {
  getDirectoryHandle: (
    n: string,
    o?: { create?: boolean },
  ) => Promise<FileSystemDirectoryHandle>;
  getFileHandle: (
    n: string,
    o?: { create?: boolean },
  ) => Promise<FileSystemFileHandle>;
};

async function fileHandleAtPath(
  root: FileSystemDirectoryHandle,
  path: string,
  create: boolean,
): Promise<FileSystemFileHandle | null> {
  try {
    const parts = path.split("/").filter(Boolean);
    if (parts.length === 0) return null;
    let dir = root as DirLike;
    for (const part of parts.slice(0, -1)) {
      dir = (await dir.getDirectoryHandle(part, { create })) as DirLike;
    }
    return await dir.getFileHandle(parts[parts.length - 1], { create });
  } catch {
    return null;
  }
}

async function pathHasFile(root: FileSystemDirectoryHandle, path: string): Promise<boolean> {
  return (await fileHandleAtPath(root, path, false)) !== null;
}

async function writeBytesAtPath(
  root: FileSystemDirectoryHandle,
  path: string,
  data: Uint8Array | string,
): Promise<boolean> {
  const fh = await fileHandleAtPath(root, path, true);
  if (!fh) return false;
  const create = (
    fh as unknown as {
      createWritable?: () => Promise<{
        write: (d: BufferSource | string) => Promise<void>;
        close: () => Promise<void>;
      }>;
    }
  ).createWritable;
  if (typeof create !== "function") return false;
  try {
    const writable = await create.call(fh);
    await writable.write(typeof data === "string" ? data : (data.slice().buffer as ArrayBuffer));
    await writable.close();
    return true;
  } catch {
    return false;
  }
}

function deployedImportPointer(): string | null {
  if (typeof window === "undefined") return null;
  const q = new URLSearchParams(window.location.search);
  if (q.get("source") === "local") return null;
  if (q.get("project")?.trim()) return null;
  if (q.get("new") === "1") return null;
  const pointer = q.get("pointer")?.trim();
  return pointer || null;
}

const materializedDirs = new Map<string, FileSystemDirectoryHandle>();

type PickOutcome =
  | { status: "picked"; dir: FileSystemDirectoryHandle }
  | { status: "canceled" }
  | { status: "unsupported" };

async function pickProjectDir(slug: string): Promise<PickOutcome> {
  const w = window as Window & {
    showDirectoryPicker?: (opts?: { mode?: "read" | "readwrite" }) => Promise<FileSystemDirectoryHandle>;
  };
  if (typeof w.showDirectoryPicker !== "function") return { status: "unsupported" };
  try {
    const parent = await w.showDirectoryPicker({ mode: "readwrite" });
    const dir = await (parent as DirLike).getDirectoryHandle(slug, { create: true });
    return { status: "picked", dir };
  } catch (e) {
    const aborted =
      !!e && typeof e === "object" && (e as DOMException).name === "AbortError";
    return { status: aborted ? "canceled" : "unsupported" };
  }
}

const FS_UNSAFE_CHAR_RE = /[<>:"\\|?*\u0000-\u001f\u007f]/g;

export function sanitizeContentPath(path: string): string {
  return path
    .split("/")
    .map((segment) => {
      const cleaned = segment
        .replace(/[\u202f\u00a0]/g, " ")
        .replace(FS_UNSAFE_CHAR_RE, "_")
        .replace(/[. ]+$/, "");
      return cleaned || "_";
    })
    .join("/");
}

export function rewriteAliasedPaths(
  text: string,
  aliased: ReadonlyMap<string, string>,
): string {
  if (aliased.size === 0) return text;
  let composite: unknown;
  try {
    composite = JSON.parse(text);
  } catch {
    return text;
  }
  const walk = (node: unknown): unknown => {
    if (typeof node === "string") return aliased.get(node) ?? node;
    if (Array.isArray(node)) return node.map(walk);
    if (node && typeof node === "object") {
      const out: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(node)) out[k] = walk(v);
      return out;
    }
    return node;
  };
  try {
    return JSON.stringify(walk(composite));
  } catch {
    return text;
  }
}

export type MirrorContentsResult = {
  failed: string[];
  aliased: Map<string, string>;
};

async function mirrorEntityContents(
  dir: FileSystemDirectoryHandle,
  contents: { file: string; hash: string }[],
): Promise<MirrorContentsResult> {
  const base = catalystBase();
  const failed: string[] = [];
  const aliased = new Map<string, string>();
  const wanted: { file: string; hash: string; alias: string | null }[] = [];
  for (const c of contents) {
    if (COMPOSITE_PATH_RE.test(c.file)) continue;
    if (await pathHasFile(dir, c.file)) continue;
    const alias = sanitizeContentPath(c.file);
    if (alias !== c.file && (await pathHasFile(dir, alias))) {
      aliased.set(c.file, alias);
      continue;
    }
    wanted.push({ file: c.file, hash: c.hash, alias: alias !== c.file ? alias : null });
  }
  for (let i = 0; i < wanted.length; i += CONTENT_FETCH_CONCURRENCY) {
    const batch = wanted.slice(i, i + CONTENT_FETCH_CONCURRENCY);
    await Promise.all(
      batch.map(async (c) => {
        try {
          const res = await fetch(`${base}/content/contents/${c.hash}`, {
            credentials: "omit",
          });
          if (!res.ok) {
            failed.push(c.file);
            return;
          }
          const data = new Uint8Array(await res.arrayBuffer());
          if (await writeBytesAtPath(dir, c.file, data)) return;
          if (c.alias && (await writeBytesAtPath(dir, c.alias, data))) {
            aliased.set(c.file, c.alias);
            return;
          }
          failed.push(c.file);
        } catch {
          failed.push(c.file);
        }
      }),
    );
  }
  return { failed, aliased };
}

async function downloadDeployedProjectZip(
  entity: DeployedEntityFiles,
  text: string,
  entities: number,
  slug: string,
): Promise<SaveSceneResult> {
  const { entries } = await fetchContentBytes(
    entity.contents.filter((c) => !COMPOSITE_PATH_RE.test(c.file)),
  );
  entries.unshift({ path: COMPOSITE_FILENAME, data: zipEnc.encode(text) });
  if (!entity.contents.some((c) => SCENE_JSON_PATH_RE.test(c.file))) {
    entries.unshift({ path: SCENE_JSON_FILENAME, data: zipEnc.encode(entity.metadataText) });
  }
  downloadBlob(
    `${slug}.zip`,
    makeBinaryZip(entries.map((e) => ({ ...e, path: `${slug}/${e.path}` }))),
    "application/zip",
  );
  return {
    written: true,
    via: "download",
    text,
    entities,
    filename: COMPOSITE_FILENAME,
  };
}

async function persistDeployedProject(
  text: string,
  entities: number,
  opts: SaveSceneOptions,
): Promise<SaveSceneResult | null> {
  if (opts.writer || opts.handle) return null;
  const pointer = deployedImportPointer();
  if (!pointer) return null;
  const entity = await fetchDeployedEntityByPointer(pointer);
  if (!entity) return null;

  const { handleStore, slugifyProjectTitle, ensureHandlePermission } = await import(
    "./handle-store"
  );
  const title = opts.project?.title?.trim() || entity.title;
  const slug = slugifyProjectTitle(title);

  let dir = materializedDirs.get(slug) ?? null;
  if (!dir) {
    const stored = await handleStore.get(slug).catch(() => null);
    if (stored && (await ensureHandlePermission(stored, "readwrite"))) dir = stored;
  }
  if (!dir) {
    const picked = await pickProjectDir(slug);
    if (picked.status === "canceled") {
      return { written: false, via: "canceled", text, entities, filename: COMPOSITE_FILENAME };
    }
    if (picked.status === "unsupported") {
      return downloadDeployedProjectZip(entity, text, entities, slug);
    }
    dir = picked.dir;
  }
  materializedDirs.set(slug, dir);

  let out = text;
  try {
    out = await persistBuilderAssets(text, dir);
  } catch {
  }

  const mirror = await mirrorEntityContents(dir, entity.contents);
  out = rewriteAliasedPaths(out, mirror.aliased);

  if (!entity.contents.some((c) => SCENE_JSON_PATH_RE.test(c.file))) {
    if (!(await pathHasFile(dir, SCENE_JSON_FILENAME))) {
      await writeBytesAtPath(dir, SCENE_JSON_FILENAME, entity.metadataText);
    }
  }
  if (!(await writeBytesAtPath(dir, COMPOSITE_FILENAME, out))) {
    return null;
  }

  await handleStore.put(slug, dir);
  await handleStore.putMeta(slug, {
    title,
    base: opts.project?.base ?? entity.base,
    composite: out,
    updatedAt: Date.now(),
  });

  if (mirror.failed.length > 0) {
    console.warn(
      `[save] could not mirror ${mirror.failed.length} content file(s) into the ` +
        `project folder: ${mirror.failed.join(", ")}`,
    );
  }

  return {
    written: true,
    via: "fsa-handle",
    text: out,
    entities,
    filename: COMPOSITE_FILENAME,
    ...(mirror.failed.length > 0 ? { contentWarnings: mirror.failed } : {}),
  };
}
