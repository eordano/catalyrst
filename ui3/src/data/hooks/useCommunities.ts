import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { qk, STALE } from "../queryKeys";
import { joinCommunity, leaveCommunity } from "../catalyst/communities";
import {
  loadCommunities,
  loadCommunity,
  loadCommunityPosts,
  loadCommunityPlaces,
} from "../catalyst/communitiesSchema";
import type { QueryParams } from "../catalyst/client";
import { useBridgeState } from "../../overlay/bridge";

export function useCommunities(params: QueryParams = {}) {
  const identity = useBridgeState(s => s.identity);
  const viewer = identity.address;
  return useQuery({
    queryKey: [...qk.communities(params), viewer],
    queryFn: ({ signal }) => loadCommunities(params, { signal, authenticated: !!viewer }),
    staleTime: STALE.communities,
  });
}

export function useCommunity(id?: string | null) {
  const identity = useBridgeState(s => s.identity);
  const viewer = identity.address;
  return useQuery({
    queryKey: [...qk.community(id), viewer],
    queryFn: ({ signal }) => loadCommunity(id, { signal, authenticated: !!viewer }),
    staleTime: STALE.community,
    enabled: Boolean(id),
  });
}

export function useCommunityPosts(id?: string | null) {
  return useQuery({
    queryKey: qk.communityPosts(id),
    queryFn: ({ signal }) => loadCommunityPosts(id, { signal }),
    staleTime: STALE.communityPosts,
    enabled: Boolean(id),
  });
}

export function useCommunityPlaces(id?: string | null) {
  return useQuery({
    queryKey: qk.communityPlaces(id),
    queryFn: ({ signal }) => loadCommunityPlaces(id, { signal }),
    staleTime: STALE.communityPlaces,
    enabled: Boolean(id),
  });
}

export function useJoinCommunity() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, privacy }: { id: string; privacy?: string }) =>
      joinCommunity(id, { privacy }),
    onSuccess: (_res, { id }) => {
      qc.invalidateQueries({ queryKey: qk.community(id) });
      qc.invalidateQueries({ queryKey: ["communities"] });
    },
  });
}

export function useLeaveCommunity() {
  const address = useBridgeState(s => s.identity.address);
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id }: { id: string }) => {
      return leaveCommunity(id, { address: address ?? undefined });
    },
    onSuccess: (_res, { id }) => {
      qc.invalidateQueries({ queryKey: qk.community(id) });
      qc.invalidateQueries({ queryKey: ["communities"] });
    },
  });
}
