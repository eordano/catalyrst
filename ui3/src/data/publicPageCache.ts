export type PublicPage<T> = {
  data?: T;
  updatedAt: number;
  pending: boolean;
  error: boolean;
};

type Entry<T> = { page: PublicPage<T>; listeners: Set<() => void>; request?: Promise<void> };
const RETAIN_MS = 5 * 60_000;
const FRESH_MS = 30_000;

export function createPublicPageCache<T>(fetchPage: (key: string) => Promise<T>) {
  const entries = new Map<string, Entry<T>>();
  const empty: PublicPage<T> = { updatedAt: 0, pending: false, error: false };
  function entry(key: string) {
    let value = entries.get(key);
    if (!value) {
      for (const [oldKey, old] of entries) {
        if (entries.size < 128) break;
        if (!old.listeners.size && !old.request) entries.delete(oldKey);
      }
      value = { page: empty, listeners: new Set() };
      entries.set(key, value);
    }
    return value;
  }
  function publish(value: Entry<T>, page: PublicPage<T>) {
    value.page = page;
    value.listeners.forEach(listener => listener());
  }
  function expire(key: string) {
    const value = entry(key);
    if (value.page.data !== undefined && Date.now() - value.page.updatedAt >= RETAIN_MS) {
      publish(value, { ...value.page, data: undefined });
    }
  }
  function load(key: string, force = false): Promise<void> {
    expire(key);
    const value = entry(key);
    if (value.request) return value.request;
    if (!force && value.page.data !== undefined && Date.now() - value.page.updatedAt < FRESH_MS) return Promise.resolve();
    publish(value, { ...value.page, pending: true, error: false });
    const request = Promise.resolve().then(() => fetchPage(key)).then(data => {
      publish(value, { data, updatedAt: Date.now(), pending: false, error: false });
    }, () => {
      expire(key);
      publish(value, { ...value.page, pending: false, error: true });
    }).finally(() => { value.request = undefined; });
    value.request = request;
    return request;
  }
  return {
    empty,
    retainMs: RETAIN_MS,
    get: (key: string) => entry(key).page,
    subscribe(key: string, listener: () => void) {
      const value = entry(key);
      value.listeners.add(listener);
      return () => { value.listeners.delete(listener); };
    },
    expire,
    load,
  };
}
