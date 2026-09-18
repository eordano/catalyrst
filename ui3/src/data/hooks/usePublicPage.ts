import { useCallback, useEffect, useSyncExternalStore } from "react";
import type { createPublicPageCache } from "../publicPageCache";

export function usePublicPage<T>(cache: ReturnType<typeof createPublicPageCache<T>>, key: string | null) {
  const subscribe = useCallback((listener: () => void) => key === null ? () => {} : cache.subscribe(key, listener), [cache, key]);
  const read = useCallback(() => key === null ? cache.empty : cache.get(key), [cache, key]);
  const page = useSyncExternalStore(subscribe, read, () => cache.empty);
  useEffect(() => {
    if (key !== null) void cache.load(key);
  }, [cache, key]);
  useEffect(() => {
    if (key === null || page.data === undefined) return;
    const timer = setTimeout(() => { cache.expire(key); void cache.load(key); }, Math.max(0, page.updatedAt + cache.retainMs - Date.now()));
    return () => clearTimeout(timer);
  }, [cache, key, page.updatedAt, page.data]);
  const expired = page.updatedAt > 0 && Date.now() - page.updatedAt >= cache.retainMs;
  const data = expired ? undefined : page.data;
  return { ...page, data, loading: key !== null && data === undefined && !page.error, refreshFailed: data !== undefined && page.error, retry: () => key === null ? undefined : cache.load(key, true) };
}
