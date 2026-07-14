import { check } from "@ui/validate";
import { isRecord } from "@ui/data/catalyst/rows";

import type { ZodType } from "zod";

export type PendingStore<T extends { ts: number }> = {
  get(signer: string | null | undefined): T | null;
  set(signer: string | null | undefined, entry: T): void;
  clear(signer: string | null | undefined): void;
};

/**
 * `storeSchema` and `boundary` come from the caller because this function is
 * shape-agnostic by design; `validate` stays a separate business rule (an entry
 * can be the right shape and still not worth resuming).
 *
 * A type-only zod import, so this module contributes nothing to the bundle a
 * perf build is trying to shed.
 */
export function createPendingStore<T extends { ts: number }>(
  key: string,
  ttlMs: number,
  validate: (entry: T) => boolean,
  storeSchema: ZodType<Record<string, T>>,
  boundary: string,
): PendingStore<T> {
  type Store = Record<string, T>;

  function keyFor(signer: string | null | undefined): string | null {
    if (!signer) return null;
    return signer.toLowerCase();
  }

  /**
   * `null` when storage could not be read or held something else.
   *
   * The try covers the read and the parse only. `check` throws in dev by
   * design, and a catch around it would collapse a drifted store into the same
   * `null` an unreadable one produces -- which is how an in-flight purchase
   * would go missing without anything saying so. The `typeof` guard stays as
   * the production fallback, since `check` returns the original value there.
   */
  function readStore(): Store | null {
    if (typeof window === "undefined") return null;
    let parsed: unknown;
    try {
      const raw = window.localStorage.getItem(key);
      if (!raw) return {};
      parsed = JSON.parse(raw);
    } catch {
      return null;
    }
    const store = check(storeSchema, parsed, boundary);
    // Arrays pass `typeof === "object"`. A stored `[]` reaching here became the
    // store, set() wrote a key onto the array, and JSON.stringify produced "[]"
    // while Object.keys().length stayed 1 -- so the write "succeeded" and the
    // in-flight purchase vanished. isRecord excludes arrays.
    return isRecord(store) ? (store as Store) : null;
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
