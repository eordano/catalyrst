const IDB_DB = "ch-project-handles";
const IDB_STORE = "handles";
const IDB_META_STORE = "meta";
const IDB_VERSION = 2;

export type ProjectMeta = {
  slug: string;
  title: string;
  updatedAt: number;
  base?: string;
  parcels?: number;
  thumbnail?: string;
  composite?: string;
  template?: string;
  codeFiles?: Record<string, string>;
  assets?: Record<string, string>;
};

export function slugifyProjectTitle(title: string): string {
  return (title || "")
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64) || "scene";
}

export type HandleBackend = {
  read(key: string): Promise<unknown>;
  write(key: string, value: unknown): Promise<void>;
  remove(key: string): Promise<void>;
  readMeta(key: string): Promise<unknown>;
  writeMeta(key: string, value: unknown): Promise<void>;
  removeMeta(key: string): Promise<void>;
  listMeta(): Promise<ProjectMeta[]>;
};

export type HandleStore = {
  put(slug: string, handle: FileSystemDirectoryHandle | null): Promise<void>;
  get(slug: string): Promise<FileSystemDirectoryHandle | null>;
  clear(slug: string): Promise<void>;
  putMeta(slug: string, meta: Partial<ProjectMeta>): Promise<void>;
  getMeta(slug: string): Promise<ProjectMeta | null>;
  list(): Promise<ProjectMeta[] | null>;
  keys(): Promise<string[] | null>;
};

function openDb(): Promise<IDBDatabase | null> {
  if (typeof indexedDB === "undefined") return Promise.resolve(null);
  return new Promise((resolve) => {
    let req: IDBOpenDBRequest;
    try {
      req = indexedDB.open(IDB_DB, IDB_VERSION);
    } catch {
      resolve(null);
      return;
    }
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(IDB_STORE)) db.createObjectStore(IDB_STORE);
      if (!db.objectStoreNames.contains(IDB_META_STORE)) db.createObjectStore(IDB_META_STORE);
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => resolve(null);
    req.onblocked = () => resolve(null);
  });
}

function idbGet(store: string, key: string): Promise<unknown> {
  return openDb().then((db) => {
    if (!db) return undefined;
    return new Promise<unknown>((resolve) => {
      const tx = db.transaction(store, "readonly");
      const r = tx.objectStore(store).get(key);
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => resolve(undefined);
    }).finally(() => db.close());
  });
}
function idbPut(store: string, key: string, value: unknown): Promise<void> {
  return openDb().then((db) => {
    if (!db) return;
    return new Promise<void>((resolve) => {
      const tx = db.transaction(store, "readwrite");
      tx.objectStore(store).put(value, key);
      tx.oncomplete = () => resolve();
      tx.onerror = () => resolve();
      tx.onabort = () => resolve();
    }).finally(() => db.close());
  });
}
function idbDelete(store: string, key: string): Promise<void> {
  return openDb().then((db) => {
    if (!db) return;
    return new Promise<void>((resolve) => {
      const tx = db.transaction(store, "readwrite");
      tx.objectStore(store).delete(key);
      tx.oncomplete = () => resolve();
      tx.onerror = () => resolve();
      tx.onabort = () => resolve();
    }).finally(() => db.close());
  });
}

export function indexedDbBackend(): HandleBackend {
  return {
    read: (key) => idbGet(IDB_STORE, key),
    write: (key, value) => idbPut(IDB_STORE, key, value),
    remove: (key) => idbDelete(IDB_STORE, key),
    readMeta: (key) => idbGet(IDB_META_STORE, key),
    writeMeta: (key, value) => idbPut(IDB_META_STORE, key, value),
    removeMeta: (key) => idbDelete(IDB_META_STORE, key),
    async listMeta() {
      const db = await openDb();
      if (!db) return [];
      try {
        return await new Promise<ProjectMeta[]>((resolve) => {
          const tx = db.transaction(IDB_META_STORE, "readonly");
          const r = tx.objectStore(IDB_META_STORE).getAll();
          r.onsuccess = () =>
            resolve(
              (Array.isArray(r.result) ? r.result : []).filter(
                (m): m is ProjectMeta => !!m && typeof (m as ProjectMeta).slug === "string",
              ),
            );
          r.onerror = () => resolve([]);
        });
      } finally {
        db.close();
      }
    },
  };
}

export function createHandleStore(backend: HandleBackend = indexedDbBackend()): HandleStore {
  return {
    async put(slug, handle) {
      if (!slug) return;
      try {
        if (handle) await backend.write(slug, handle);
        else await backend.remove(slug);
      } catch {
      }
    },
    async get(slug) {
      if (!slug) return null;
      try {
        const value = await backend.read(slug);
        return (value as FileSystemDirectoryHandle | undefined) ?? null;
      } catch {
        return null;
      }
    },
    async clear(slug) {
      if (!slug) return;
      try {
        await backend.remove(slug);
      } catch {
      }
      try {
        await backend.removeMeta(slug);
      } catch {
      }
    },
    async putMeta(slug, meta) {
      if (!slug) return;
      try {
        const prev = ((await backend.readMeta(slug)) as ProjectMeta | undefined) ?? undefined;
        const merged: ProjectMeta = {
          slug,
          title: meta.title ?? prev?.title ?? slug,
          updatedAt: meta.updatedAt ?? Date.now(),
          base: meta.base ?? prev?.base,
          parcels: meta.parcels ?? prev?.parcels,
          thumbnail: meta.thumbnail ?? prev?.thumbnail,
          composite: meta.composite ?? prev?.composite,
          template: meta.template ?? prev?.template,
          codeFiles: meta.codeFiles
            ? { ...prev?.codeFiles, ...meta.codeFiles }
            : prev?.codeFiles,
          assets: meta.assets ? { ...prev?.assets, ...meta.assets } : prev?.assets,
        };
        await backend.writeMeta(slug, merged);
      } catch {
      }
    },
    async getMeta(slug) {
      if (!slug) return null;
      try {
        return ((await backend.readMeta(slug)) as ProjectMeta | undefined) ?? null;
      } catch {
        return null;
      }
    },
    // A storage backend that threw has not told us the wallet has no projects.
    // `[]` here is the same claim as a real empty store, so it is null instead.
    async list() {
      try {
        const all = await backend.listMeta();
        return all.slice().sort((a, b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0));
      } catch {
        return null;
      }
    },
    async keys() {
      try {
        return (await backend.listMeta()).map((m) => m.slug);
      } catch {
        return null;
      }
    },
  };
}

export const handleStore: HandleStore = createHandleStore();

export async function ensureHandlePermission(
  handle: FileSystemHandle,
  mode: "read" | "readwrite" = "read",
): Promise<boolean> {
  const h = handle as unknown as {
    queryPermission?: (d: { mode: string }) => Promise<PermissionState>;
    requestPermission?: (d: { mode: string }) => Promise<PermissionState>;
  };
  try {
    if (typeof h.queryPermission === "function") {
      if ((await h.queryPermission({ mode })) === "granted") return true;
    }
    if (typeof h.requestPermission === "function") {
      return (await h.requestPermission({ mode })) === "granted";
    }
  } catch {
  }
  return true;
}
