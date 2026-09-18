import { useCallback, useEffect, useReducer, useSyncExternalStore } from "react";
import { useQueryClient, type QueryKey, type UseQueryResult } from "@tanstack/react-query";
import type { PublicFreshness } from "../screens/section";

export const publicFreshnessKey = (key: QueryKey) => ["public-freshness", ...key];
const MAX_AGE = 5 * 60_000;

export function usePublicResult<T>(query: UseQueryResult<T>, key: QueryKey, enabled = true, sourceKey: QueryKey = key) {
  const client = useQueryClient();
  const serialized = JSON.stringify(key);
  const source = JSON.stringify(sourceKey);
  const snapshot = useCallback(() => client.getQueryData<PublicFreshness>(publicFreshnessKey(JSON.parse(source))), [client, source]);
  const freshness = useSyncExternalStore(client.getQueryCache().subscribe, snapshot, snapshot);
  const [, tick] = useReducer(value => value + 1, 0);
  const updatedAt = freshness?.updatedAt ?? query.dataUpdatedAt;
  const expiresIn = updatedAt + MAX_AGE - Date.now();
  const expired = enabled && query.data !== undefined && expiresIn <= 0;
  useEffect(() => {
    if (!enabled || !updatedAt || expiresIn <= 0) return;
    const timer = setTimeout(() => {
      tick();
      void client.invalidateQueries({ queryKey: JSON.parse(serialized), exact: true });
    }, expiresIn + 1);
    return () => clearTimeout(timer);
  }, [client, enabled, updatedAt, expiresIn, serialized]);
  useEffect(() => {
    if (!enabled || !freshness?.refreshing || query.isFetching) return;
    const timer = setTimeout(() => void client.invalidateQueries({ queryKey: JSON.parse(serialized), exact: true }), 1_000);
    return () => clearTimeout(timer);
  }, [client, enabled, freshness, query.isFetching, serialized]);
  const retained = enabled && query.data !== undefined && !expired;
  return {
    ...query,
    data: expired ? undefined : query.data,
    isPending: query.isPending || (expired && query.isFetching),
    isSuccess: query.isSuccess && !expired,
    isError: query.isError && !retained,
    refreshFailed: enabled && retained && Boolean(query.isError || freshness?.refreshFailed),
  };
}
