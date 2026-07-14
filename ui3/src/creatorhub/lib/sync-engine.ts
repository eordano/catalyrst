import { useSyncExternalStore } from "react";

export type SyncState =
  | "synced"
  | "editing"
  | "pending"
  | "syncing"
  | "offline"
  | "conflict"
  | "error";

export type SceneSyncStatus = {
  id: string;
  state: SyncState;
  version: number;
  dirtyAt: number | null;
  lastSyncedAt: number | null;
};

export type EngineSummary = {
  overall: SyncState;
  pending: number;
  scenes: SceneSyncStatus[];
  online: boolean;
};

export type PushBody = { baseVersion: number; hash: string; blob: unknown; title?: string };
export type PushResult = { ok: boolean; conflict?: boolean; version?: number; server?: unknown };
export type PushFn = (id: string, body: PushBody) => Promise<PushResult>;

export type SyncEngine = {
  subscribe(fn: (s: EngineSummary) => void): () => void;
  getSummary(): EngineSummary;
  markEditing(id: string): void;
  saveLocal(id: string, blob: unknown, title?: string): Promise<void>;
  setTransport(pushFn: PushFn): void;
  resolveConflict(id: string, choice: "local" | "cloud"): Promise<void>;
  start(): void;
  stop(): void;
};

type Rec = {
  id: string;
  blob: unknown;
  title?: string;
  version: number;
  hash: string;
  syncedHash: string | null;
  dirtyAt: number | null;
  lastSyncedAt: number | null;
  state: SyncState;
  serverVersion?: number;
  serverBlob?: unknown;
};

const RETRY_MS = 5000;

function stableStringify(value: unknown, seen: Set<unknown> = new Set()): string {
  if (value === null || typeof value !== "object") {
    const s = JSON.stringify(value);
    return s === undefined ? "null" : s;
  }
  if (seen.has(value)) return '"[Circular]"';
  seen.add(value);
  let out: string;
  if (Array.isArray(value)) {
    out = "[" + value.map((v) => stableStringify(v, seen)).join(",") + "]";
  } else {
    const obj = value as Record<string, unknown>;
    const keys = Object.keys(obj).sort();
    out =
      "{" +
      keys.map((k) => JSON.stringify(k) + ":" + stableStringify(obj[k], seen)).join(",") +
      "}";
  }
  seen.delete(value);
  return out;
}

export function hashBlob(blob: unknown): string {
  const s = stableStringify(blob);
  let h = 5381;
  for (let i = 0; i < s.length; i++) {
    h = ((h << 5) + h + s.charCodeAt(i)) | 0;
  }
  return (h >>> 0).toString(16).padStart(8, "0") + "-" + (s.length >>> 0).toString(16);
}

interface Persist {
  loadAll(): Promise<{ drafts: Rec[]; outbox: string[] }>;
  putDraft(rec: Rec): Promise<void>;
  deleteDraft(id: string): Promise<void>;
  putOutbox(ids: string[]): Promise<void>;
}

function memoryPersist(): Persist {
  return {
    loadAll: async () => ({ drafts: [], outbox: [] }),
    putDraft: async () => {},
    deleteDraft: async () => {},
    putOutbox: async () => {},
  };
}

function idbPersist(dbName: string): Persist {
  let dbp: Promise<IDBDatabase> | null = null;
  const open = (): Promise<IDBDatabase> => {
    if (!dbp) {
      dbp = new Promise<IDBDatabase>((resolve, reject) => {
        const req = indexedDB.open(dbName, 1);
        req.onupgradeneeded = () => {
          const d = req.result;
          if (!d.objectStoreNames.contains("drafts")) d.createObjectStore("drafts", { keyPath: "id" });
          if (!d.objectStoreNames.contains("meta")) d.createObjectStore("meta");
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error ?? new Error("idb open failed"));
      });
      dbp.catch(() => {
        dbp = null;
      });
    }
    return dbp;
  };
  const run = <T>(
    store: string,
    mode: IDBTransactionMode,
    fn: (s: IDBObjectStore) => IDBRequest<T>,
  ): Promise<T> =>
    open().then(
      (d) =>
        new Promise<T>((resolve, reject) => {
          let req: IDBRequest<T>;
          try {
            const t = d.transaction(store, mode);
            req = fn(t.objectStore(store));
          } catch (e) {
            reject(e);
            return;
          }
          req.onsuccess = () => resolve(req.result);
          req.onerror = () => reject(req.error ?? new Error("idb op failed"));
        }),
    );
  return {
    loadAll: async () => {
      try {
        const drafts = await run<Rec[]>("drafts", "readonly", (s) => s.getAll() as IDBRequest<Rec[]>);
        const outbox = await run<string[] | undefined>(
          "meta",
          "readonly",
          (s) => s.get("outbox") as IDBRequest<string[] | undefined>,
        );
        return { drafts: drafts ?? [], outbox: outbox ?? [] };
      } catch {
        return { drafts: [], outbox: [] };
      }
    },
    putDraft: async (rec) => {
      try {
        await run("drafts", "readwrite", (s) => s.put(rec));
      } catch {
      }
    },
    deleteDraft: async (id) => {
      try {
        await run("drafts", "readwrite", (s) => s.delete(id));
      } catch {
      }
    },
    putOutbox: async (ids) => {
      try {
        await run("meta", "readwrite", (s) => s.put(ids.slice(), "outbox"));
      } catch {
      }
    },
  };
}

export function createSyncEngine(opts?: { dbName?: string }): SyncEngine {
  const dbName = opts?.dbName ?? "dcl-creatorhub-sync";
  const hasIdb = typeof indexedDB !== "undefined";
  const store: Persist = hasIdb ? idbPersist(dbName) : memoryPersist();

  const recs = new Map<string, Rec>();
  const outbox: string[] = [];
  const listeners = new Set<(s: EngineSummary) => void>();

  let transport: PushFn | null = null;
  let online =
    typeof navigator !== "undefined" && typeof navigator.onLine === "boolean" ? navigator.onLine : true;
  let running = false;
  let flushing = false;
  let flushQueued = false;
  let timer: ReturnType<typeof setInterval> | null = null;
  let summary: EngineSummary;

  const now = () => Date.now();

  const scenesList = (): SceneSyncStatus[] =>
    Array.from(recs.values()).map((r) => ({
      id: r.id,
      state: r.state,
      version: r.version,
      dirtyAt: r.dirtyAt,
      lastSyncedAt: r.lastSyncedAt,
    }));

  const computeOverall = (): SyncState => {
    let conflict = false;
    let syncing = false;
    let error = false;
    let editing = false;
    for (const r of recs.values()) {
      if (r.state === "conflict") conflict = true;
      else if (r.state === "syncing") syncing = true;
      else if (r.state === "error") error = true;
      else if (r.state === "editing") editing = true;
    }
    if (conflict) return "conflict";
    if (syncing) return "syncing";
    if (!online && outbox.length > 0) return "offline";
    if (error) return "error";
    if (outbox.length > 0) return "pending";
    if (editing) return "editing";
    return "synced";
  };

  const recompute = () => {
    summary = { overall: computeOverall(), pending: outbox.length, scenes: scenesList(), online };
  };

  const notify = () => {
    recompute();
    for (const l of listeners) l(summary);
  };

  recompute();

  const persistRec = (r: Rec) => store.putDraft(r);
  const persistOutbox = () => {
    void store.putOutbox(outbox);
  };

  const addToOutbox = (id: string) => {
    if (!outbox.includes(id)) {
      outbox.push(id);
      persistOutbox();
    }
  };
  const removeFromOutbox = (id: string) => {
    const i = outbox.indexOf(id);
    if (i >= 0) {
      outbox.splice(i, 1);
      persistOutbox();
    }
  };

  const ensureRec = (id: string): Rec => {
    let r = recs.get(id);
    if (!r) {
      r = {
        id,
        blob: undefined,
        version: 0,
        hash: "",
        syncedHash: null,
        dirtyAt: null,
        lastSyncedAt: null,
        state: "synced",
      };
      recs.set(id, r);
    }
    return r;
  };

  const scheduleFlush = () => {
    if (typeof queueMicrotask !== "undefined") queueMicrotask(() => void flush());
    else setTimeout(() => void flush(), 0);
  };

  const flush = async (): Promise<void> => {
    if (flushing) {
      flushQueued = true;
      return;
    }
    if (!transport || !online || outbox.length === 0) return;
    const tx = transport;
    flushing = true;
    try {
      while (online && outbox.length > 0) {
        const id = outbox[0];
        if (id === undefined) break;
        const rec = recs.get(id);
        if (!rec) {
          removeFromOutbox(id);
          continue;
        }
        rec.state = "syncing";
        void persistRec(rec);
        notify();
        const pushedHash = rec.hash;
        const baseVersion = rec.version;
        let result: PushResult;
        try {
          result = await tx(id, {
            baseVersion,
            hash: pushedHash,
            blob: rec.blob,
            title: rec.title,
          });
        } catch {
          rec.state = online ? "error" : "offline";
          void persistRec(rec);
          notify();
          break;
        }
        const cur = recs.get(id);
        if (!cur) {
          removeFromOutbox(id);
          continue;
        }
        if (result.ok) {
          cur.version = typeof result.version === "number" ? result.version : baseVersion + 1;
          cur.lastSyncedAt = now();
          cur.syncedHash = pushedHash;
          removeFromOutbox(id);
          if (cur.hash === pushedHash) {
            cur.dirtyAt = null;
            cur.state = "synced";
          } else {
            cur.state = online ? "pending" : "offline";
            addToOutbox(id);
          }
          void persistRec(cur);
          notify();
        } else if (result.conflict) {
          cur.state = "conflict";
          cur.serverVersion = typeof result.version === "number" ? result.version : cur.version;
          cur.serverBlob = result.server;
          removeFromOutbox(id);
          void persistRec(cur);
          notify();
        } else {
          cur.state = "error";
          void persistRec(cur);
          notify();
          break;
        }
      }
    } finally {
      flushing = false;
      if (flushQueued) {
        flushQueued = false;
        scheduleFlush();
      }
    }
  };

  const saveLocal = async (id: string, blob: unknown, title?: string): Promise<void> => {
    const hash = hashBlob(blob);
    const rec = ensureRec(id);
    if (title !== undefined) rec.title = title;
    if (
      rec.hash === hash &&
      rec.syncedHash === hash &&
      rec.state !== "conflict" &&
      !outbox.includes(id)
    ) {
      rec.blob = blob;
      rec.state = "synced";
      await persistRec(rec);
      notify();
      return;
    }
    rec.blob = blob;
    rec.hash = hash;
    rec.dirtyAt = now();
    rec.state = online ? "pending" : "offline";
    addToOutbox(id);
    await persistRec(rec);
    notify();
    scheduleFlush();
  };

  const markEditing = (id: string): void => {
    const rec = ensureRec(id);
    if (rec.state === "conflict") return;
    rec.state = "editing";
    void persistRec(rec);
    notify();
  };

  const resolveConflict = async (id: string, choice: "local" | "cloud"): Promise<void> => {
    const rec = recs.get(id);
    if (!rec || rec.state !== "conflict") return;
    if (choice === "cloud") {
      rec.blob = rec.serverBlob;
      rec.version = rec.serverVersion ?? rec.version;
      rec.hash = hashBlob(rec.blob);
      rec.syncedHash = rec.hash;
      rec.dirtyAt = null;
      rec.lastSyncedAt = now();
      rec.serverBlob = undefined;
      rec.serverVersion = undefined;
      rec.state = "synced";
      removeFromOutbox(id);
      await persistRec(rec);
      notify();
      return;
    }
    rec.version = rec.serverVersion ?? rec.version;
    rec.serverBlob = undefined;
    rec.serverVersion = undefined;
    rec.dirtyAt = rec.dirtyAt ?? now();
    rec.state = online ? "pending" : "offline";
    addToOutbox(id);
    await persistRec(rec);
    notify();
    scheduleFlush();
  };

  const setTransport = (pushFn: PushFn): void => {
    transport = pushFn;
    scheduleFlush();
  };

  const normalizeRec = (sr: Rec): Rec => ({
    id: sr.id,
    blob: sr.blob,
    title: sr.title,
    version: typeof sr.version === "number" ? sr.version : 0,
    hash: sr.hash ?? "",
    syncedHash: sr.syncedHash ?? null,
    dirtyAt: sr.dirtyAt ?? null,
    lastSyncedAt: sr.lastSyncedAt ?? null,
    state: sr.state === "syncing" ? "pending" : (sr.state ?? "synced"),
    serverVersion: sr.serverVersion,
    serverBlob: sr.serverBlob,
  });

  const hydrate = async (): Promise<void> => {
    const data = await store.loadAll();
    let changed = false;
    for (const sr of data.drafts) {
      if (sr && typeof sr.id === "string" && !recs.has(sr.id)) {
        recs.set(sr.id, normalizeRec(sr));
        changed = true;
      }
    }
    for (const id of data.outbox) {
      if (recs.has(id) && !outbox.includes(id)) {
        outbox.push(id);
        changed = true;
      }
    }
    if (changed) {
      notify();
      scheduleFlush();
    }
  };

  const onOnline = () => {
    online = true;
    for (const id of outbox) {
      const r = recs.get(id);
      if (r && r.state !== "conflict") r.state = "pending";
    }
    notify();
    scheduleFlush();
  };
  const onOffline = () => {
    online = false;
    for (const id of outbox) {
      const r = recs.get(id);
      if (r && r.state !== "conflict" && r.state !== "syncing") r.state = "offline";
    }
    notify();
  };

  const start = (): void => {
    if (running) return;
    running = true;
    if (typeof window !== "undefined" && typeof window.addEventListener === "function") {
      window.addEventListener("online", onOnline);
      window.addEventListener("offline", onOffline);
    }
    if (typeof setInterval !== "undefined") {
      timer = setInterval(() => {
        if (online && transport && outbox.length > 0 && !flushing) scheduleFlush();
      }, RETRY_MS);
    }
    scheduleFlush();
  };

  const stop = (): void => {
    running = false;
    if (typeof window !== "undefined" && typeof window.removeEventListener === "function") {
      window.removeEventListener("online", onOnline);
      window.removeEventListener("offline", onOffline);
    }
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  };

  void hydrate();

  return {
    subscribe: (fn) => {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
    getSummary: () => summary,
    markEditing,
    saveLocal,
    setTransport,
    resolveConflict,
    start,
    stop,
  };
}

export function useSyncSummary(engine: SyncEngine): EngineSummary {
  return useSyncExternalStore(engine.subscribe, engine.getSummary, engine.getSummary);
}
