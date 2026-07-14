import {
  emptySeed,
  seedFromCompositeJSON,
  type SceneEditorSeed,
} from "../catalyst/creator-hub/scene-editor";
import {
  openDirectoryOutcome,
  openFiles,
  readDirectoryFiles,
  readText,
} from "./disk";

const SEED_KEY = "ch:local-scene-seed";
const COMPOSITE_KEY = "ch:local-scene-composite";
const IDB_DB = "ch-fs";
const IDB_STORE = "handles";
const HANDLE_KEY = "local-composite-handle";

const COMPOSITE_RE = /(^|\/)main\.composite$/i;
const SCENE_JSON_RE = /(^|\/)scene\.json$/i;
const GLB_RE = /\.(glb|gltf)$/i;

export type LocalSceneResult = {
  seed: SceneEditorSeed;
  compositeHandle: FileSystemFileHandle | null;
  compositeText: string;
  source: "directory" | "file";
  files: Record<string, File> | null;
  sceneJsonText: string | null;
};

function safeJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function sceneMeta(text: string | null): {
  title?: string;
  base?: string;
  parcels?: string[];
  template?: string;
} {
  if (!text) return {};
  const j = safeJson(text) as
    | {
        display?: { title?: string };
        scene?: { base?: string; parcels?: string[] };
        tags?: string[];
      }
    | null;
  const tag = Array.isArray(j?.tags) ? j?.tags?.find((t) => typeof t === "string" && t) : undefined;
  return {
    title: j?.display?.title,
    base: j?.scene?.base,
    parcels: j?.scene?.parcels,
    template: tag || undefined,
  };
}

function baseSeedFor(meta: {
  title?: string;
  base?: string;
  parcels?: string[];
  template?: string;
}): SceneEditorSeed {
  const base = meta.base ?? meta.parcels?.[0] ?? "0,0";
  const seed = emptySeed(base);
  seed.scene = {
    ...seed.scene,
    title: meta.title?.trim() || "Local scene",
    base,
    parcels: meta.parcels?.length ? meta.parcels : seed.scene.parcels,
    ...(meta.template ? { template: meta.template } : {}),
  };
  return seed;
}

async function fileHandleAt(
  dir: FileSystemDirectoryHandle,
  path: string,
): Promise<FileSystemFileHandle | null> {
  try {
    const parts = path.split("/");
    let cur = dir;
    for (let i = 0; i < parts.length - 1; i += 1) {
      cur = await cur.getDirectoryHandle(parts[i]);
    }
    return await cur.getFileHandle(parts[parts.length - 1]);
  } catch {
    return null;
  }
}

export type OpenLocalSceneOutcome =
  | { status: "opened"; result: LocalSceneResult }
  | { status: "cancelled" }
  | { status: "no-composite"; fileCount: number; hasSceneJson: boolean };

export type OpenLocalScenePhase = "picking" | "reading";

export async function openLocalScene(opts?: {
  onPhase?: (phase: OpenLocalScenePhase) => void;
}): Promise<OpenLocalSceneOutcome> {
  opts?.onPhase?.("picking");
  const dirOutcome = await openDirectoryOutcome("readwrite");
  if (dirOutcome.status === "cancelled") return { status: "cancelled" };

  if (dirOutcome.status === "opened") {
    const dir = dirOutcome.handle;
    opts?.onPhase?.("reading");
    const files = await readDirectoryFiles(dir);
    const paths = Object.keys(files);
    const compPath = paths.find((p) => COMPOSITE_RE.test(p));
    if (!compPath) {
      return {
        status: "no-composite",
        fileCount: paths.length,
        hasSceneJson: paths.some((p) => SCENE_JSON_RE.test(p)),
      };
    }
    const sjPath = paths.find((p) => SCENE_JSON_RE.test(p));
    const sceneJsonText = sjPath ? await readText(files[sjPath]) : null;
    const meta = sceneMeta(sceneJsonText);
    const compositeText = await readText(files[compPath]);
    const json = safeJson(compositeText);
    const glbs = paths.filter((p) => GLB_RE.test(p));
    const seed = seedFromCompositeJSON(json, baseSeedFor(meta), glbs);
    return {
      status: "opened",
      result: {
        seed,
        compositeHandle: await fileHandleAt(dir, compPath),
        compositeText,
        source: "directory",
        files,
        sceneJsonText,
      },
    };
  }

  const picked = await openFiles({
    accept: { "application/json": [".composite", ".json"] },
    description: "Scene composite (main.composite)",
  });
  const comp = picked.find((p) => COMPOSITE_RE.test(p.name)) ?? picked[0];
  if (!comp) return { status: "cancelled" };
  opts?.onPhase?.("reading");
  const compositeText = await readText(comp.file);
  const json = safeJson(compositeText);
  const seed = seedFromCompositeJSON(
    json,
    baseSeedFor({ title: comp.name.replace(/\.composite$/i, "") }),
  );
  return {
    status: "opened",
    result: {
      seed,
      compositeHandle: comp.handle,
      compositeText,
      source: "file",
      files: null,
      sceneJsonText: null,
    },
  };
}


export function stashLocalSeed(seed: SceneEditorSeed): void {
  if (typeof sessionStorage === "undefined") return;
  try {
    sessionStorage.setItem(SEED_KEY, JSON.stringify(seed));
  } catch {
  }
}

export function readLocalSeed(): SceneEditorSeed | null {
  if (typeof sessionStorage === "undefined") return null;
  try {
    const raw = sessionStorage.getItem(SEED_KEY);
    return raw ? (JSON.parse(raw) as SceneEditorSeed) : null;
  } catch {
    return null;
  }
}

export function clearLocalSeed(): void {
  if (typeof sessionStorage !== "undefined") {
    try {
      sessionStorage.removeItem(SEED_KEY);
    } catch {
    }
  }
}


export function stashLocalComposite(text: string | null | undefined): void {
  if (typeof sessionStorage === "undefined") return;
  try {
    if (typeof text === "string" && text.trim() !== "") {
      sessionStorage.setItem(COMPOSITE_KEY, text);
    } else {
      sessionStorage.removeItem(COMPOSITE_KEY);
    }
  } catch {
  }
}

export function readLocalComposite(): string | null {
  if (typeof sessionStorage === "undefined") return null;
  try {
    return sessionStorage.getItem(COMPOSITE_KEY);
  } catch {
    return null;
  }
}

export function clearLocalComposite(): void {
  if (typeof sessionStorage !== "undefined") {
    try {
      sessionStorage.removeItem(COMPOSITE_KEY);
    } catch {
    }
  }
}


function openIdb(): Promise<IDBDatabase | null> {
  if (typeof indexedDB === "undefined") return Promise.resolve(null);
  return new Promise((resolve) => {
    const req = indexedDB.open(IDB_DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(IDB_STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => resolve(null);
  });
}

export async function stashCompositeHandle(handle: FileSystemFileHandle | null): Promise<void> {
  const db = await openIdb();
  if (!db) return;
  await new Promise<void>((resolve) => {
    const tx = db.transaction(IDB_STORE, "readwrite");
    if (handle) tx.objectStore(IDB_STORE).put(handle, HANDLE_KEY);
    else tx.objectStore(IDB_STORE).delete(HANDLE_KEY);
    tx.oncomplete = () => resolve();
    tx.onerror = () => resolve();
  });
}

export async function readCompositeHandle(): Promise<FileSystemFileHandle | null> {
  const db = await openIdb();
  if (!db) return null;
  return new Promise((resolve) => {
    const tx = db.transaction(IDB_STORE, "readonly");
    const r = tx.objectStore(IDB_STORE).get(HANDLE_KEY);
    r.onsuccess = () => resolve((r.result as FileSystemFileHandle) ?? null);
    r.onerror = () => resolve(null);
  });
}
