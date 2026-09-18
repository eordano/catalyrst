import { createPublicPageCache } from "@ui/data/publicPageCache";
import { usePublicPage } from "@ui/data/hooks/usePublicPage";
import { useEffect, useState } from "react";
import { api } from "./api";
import type { World } from "./Worlds";
import type { SocialEvent } from "./Events";

type DiscoveryPage<T> = { data: T[]; more: boolean };
export const publicWorlds = createPublicPageCache(async key => {
  const result = await api<{ data: World[]; total: number }>(`/worlds?${key}`);
  return { data: result.data, more: Number(new URLSearchParams(key).get("offset")) + result.data.length < result.total };
});
export const publicEvents = createPublicPageCache(async key => {
  const result = await api<{ data: SocialEvent[] | { events: SocialEvent[] } }>(`/events?${key}`);
  const data = Array.isArray(result.data) ? result.data : result.data.events;
  return { data: data.filter(event => Number.isFinite(Date.parse(event.next_start_at || event.start_at)) && Number.isFinite(Date.parse(event.next_finish_at || event.finish_at))), more: data.length === 24 };
});
export function warmPublicDiscovery() {
  void publicWorlds.load("search=&offset=0");
  void publicEvents.load("search=&offset=0");
}

export function usePublicDiscovery<T extends { id: string }>(cache: ReturnType<typeof createPublicPageCache<DiscoveryPage<T>>>, search: string, enabled = true) {
  const [cursor, setCursor] = useState({ search, offset: 0 });
  const [settledSearch, setSettledSearch] = useState(search);
  const [previous, setPrevious] = useState<{ search: string; data: T[]; updatedAt: number }>({ search, data: [], updatedAt: 0 });
  useEffect(() => {
    const timer = setTimeout(() => setSettledSearch(search), search ? 250 : 0);
    return () => clearTimeout(timer);
  }, [search]);
  const offset = cursor.search === search ? cursor.offset : 0;
  const page = usePublicPage(cache, enabled && search === settledSearch ? `search=${encodeURIComponent(search)}&offset=${offset}` : null);
  const old = previous.search === search && offset > 0 && Date.now() - previous.updatedAt < cache.retainMs ? previous.data : [];
  useEffect(() => {
    if (!previous.data.length) return;
    const timer = setTimeout(() => setPrevious(value => value === previous ? { ...value, data: [] } : value), Math.max(0, previous.updatedAt + cache.retainMs - Date.now()));
    return () => clearTimeout(timer);
  }, [cache.retainMs, previous]);
  const data = page.data ? [...old, ...page.data.data.filter(item => !old.some(other => other.id === item.id))] : old;
  function loadMore() {
    setPrevious({ search, data, updatedAt: old.length ? Math.min(previous.updatedAt, page.updatedAt) : page.updatedAt });
    setCursor({ search, offset: offset + 24 });
  }
  return { ...page, data, loading: enabled && (search !== settledSearch || page.loading), more: page.data?.more ?? false, loadMore };
}
