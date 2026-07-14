export type PendingStore<T extends { ts: number }> = {
  get(signer: string | null | undefined): T | null;
  set(signer: string | null | undefined, entry: T): void;
  clear(signer: string | null | undefined): void;
};

export function createPendingStore<T extends { ts: number }>(
  key: string,
  ttlMs: number,
  validate: (entry: T) => boolean,
): PendingStore<T> {
  type Store = Record<string, T>;

  function keyFor(signer: string | null | undefined): string | null {
    if (!signer) return null;
    return signer.toLowerCase();
  }

  /** `null` when storage could not be read or held something else. */
  function readStore(): Store | null {
    if (typeof window === "undefined") return null;
    try {
      const raw = window.localStorage.getItem(key);
      if (!raw) return {};
      const parsed = JSON.parse(raw) as Store;
      return parsed && typeof parsed === "object" ? parsed : null;
    } catch {
      return null;
    }
  }

  function writeStore(store: Store): void {
    if (typeof window === "undefined") return;
    try {
      if (Object.keys(store).length === 0) {
        window.localStorage.removeItem(key);
      } else {
        window.localStorage.setItem(key, JSON.stringify(store));
      }
    } catch {
    }
  }

  function clear(signer: string | null | undefined): void {
    const k = keyFor(signer);
    if (!k) return;
    const store = readStore();
    if (store && k in store) {
      delete store[k];
      writeStore(store);
    }
  }

  return {
    get(signer) {
      const k = keyFor(signer);
      if (!k) return null;
      const store = readStore();
      if (!store) return null;
      const entry = store[k];
      if (!entry || !validate(entry)) return null;
      if (typeof entry.ts === "number" && Date.now() - entry.ts > ttlMs) {
        clear(signer);
        return null;
      }
      return entry;
    },
    set(signer, entry) {
      const k = keyFor(signer);
      if (!k) return;
      // Unreadable storage cannot be merged into, so this starts a fresh one.
      const store = readStore() ?? {};
      store[k] = entry;
      writeStore(store);
    },
    clear,
  };
}
