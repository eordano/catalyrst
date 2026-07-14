const DEFAULT_MAX_ENTRIES = 24;
const DEFAULT_TTL_MS = 60_000;

export type SearchCache<T> = {
  key(request: Request): string;
  get(key: string): T | undefined;
  set(key: string, value: T): void;
};

export function makeSearchCache<T>(
  maxEntries = DEFAULT_MAX_ENTRIES,
  ttlMs = DEFAULT_TTL_MS,
): SearchCache<T> {
  const map = new Map<string, { at: number; value: T }>();
  return {
    key(request: Request): string {
      return new URL(request.url).search;
    },
    get(key: string): T | undefined {
      const entry = map.get(key);
      if (entry === undefined) return undefined;
      if (Date.now() - entry.at > ttlMs) {
        map.delete(key);
        return undefined;
      }
      map.delete(key);
      map.set(key, entry);
      return entry.value;
    },
    set(key: string, value: T): void {
      map.set(key, { at: Date.now(), value });
      while (map.size > maxEntries) {
        const oldest = map.keys().next().value as string | undefined;
        if (oldest === undefined) break;
        map.delete(oldest);
      }
    },
  };
}
