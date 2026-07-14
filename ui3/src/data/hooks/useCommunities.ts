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
import { getDeployIdentity } from "../../overlay/bridge";

export function useCommunities(params: QueryParams = {}) {
  return useQuery({
    queryKey: qk.communities(params),
    queryFn: ({ signal }) => loadCommunities(params, { signal }),
    staleTime: STALE.communities,
  });
}

export function useCommunity(id?: string | null) {
  return useQuery({
    queryKey: qk.community(id),
    queryFn: ({ signal }) => loadCommunity(id, { signal }),
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
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id }: { id: string }) => {
      const ident = getDeployIdentity();
      return leaveCommunity(id, { address: ident?.signerAddress });
    },
    onSuccess: (_res, { id }) => {
      qc.invalidateQueries({ queryKey: qk.community(id) });
      qc.invalidateQueries({ queryKey: ["communities"] });
    },
  });
}
