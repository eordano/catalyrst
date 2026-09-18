export type PublicFeed<T> = {
  data: T;
  updatedAt: number;
  refreshing: boolean;
  refreshFailed: boolean;
};

export const PUBLIC_FEED_MAX_AGE_MS = 5 * 60_000;

export function publicFeedMemo<A extends unknown[], T>(options: {
  load: (...args: A) => Promise<T>;
  keyOf?: (...args: A) => string;
  freshMs?: number;
}) {
  type Entry = { data: T; updatedAt: number; failedAt?: number };
  const entries = new Map<string, Entry>();
  const pending = new Map<string, Promise<Entry>>();
  let generation = 0;
  const read = async (...args: A): Promise<PublicFeed<T>> => {
    const key = options.keyOf?.(...args) ?? "";
    const now = Date.now();
    const cached = entries.get(key);
    const age = cached ? now - cached.updatedAt : Infinity;
    if (cached && age < (options.freshMs ?? 30_000)) {
      return { data: cached.data, updatedAt: cached.updatedAt, refreshing: false, refreshFailed: false };
    }
    const reusable = cached && age < PUBLIC_FEED_MAX_AGE_MS;
    let refresh = pending.get(key);
    if (!refresh && (!reusable || !cached.failedAt || now - cached.failedAt >= 10_000)) {
      const started = generation;
      refresh = Promise.resolve().then(() => options.load(...args)).then(data => {
        const entry = { data, updatedAt: Date.now() };
        if (generation === started) {
          entries.delete(key);
          entries.set(key, entry);
          if (entries.size > 128) entries.delete(entries.keys().next().value!);
        }
        return entry;
      }).catch(error => {
        if (cached && generation === started) cached.failedAt = Date.now();
        throw error;
      }).finally(() => {
        if (generation === started) pending.delete(key);
      });
      pending.set(key, refresh);
      void refresh.catch(() => {});
    }
    if (reusable) return {
      data: cached.data, updatedAt: cached.updatedAt,
      refreshing: Boolean(refresh), refreshFailed: cached.failedAt !== undefined,
    };
    const entry = await refresh!;
    return { data: entry.data, updatedAt: entry.updatedAt, refreshing: false, refreshFailed: false };
  };
  read.reset = () => { generation++; entries.clear(); pending.clear(); };
  return read;
}
