import { siteUrl } from "../../data/site";
import { useQuery, type QueryClient } from "@tanstack/react-query";

import { qk, STALE } from "../queryKeys";
import { FALLBACK_STATE, getDeployIdentity } from "../../overlay/bridge";
import {
  fetchProfile,
  fetchUserBadges,
  fetchUserPhotos,
  mapProfile,
  profileFaceUrl,
  normalizeAddress,
} from "../catalyst/profile";

export function resolveSelfAddress(): string {
  const id = getDeployIdentity();
  if (id?.signerAddress) return normalizeAddress(id.signerAddress);
  return normalizeAddress(FALLBACK_STATE.identity.address);
}

export function useProfile(address?: string | null) {
  const addr = address ?? resolveSelfAddress();
  return useQuery({
    queryKey: qk.profile(addr),
    queryFn: ({ signal }) => fetchProfile(addr, { signal }),
    staleTime: STALE.profile,
    enabled: Boolean(addr),
  });
}

export function useUserBadges(address?: string | null) {
  const addr = address ?? resolveSelfAddress();
  return useQuery({
    queryKey: qk.badges(addr),
    queryFn: ({ signal }) => fetchUserBadges(addr, { signal }),
    staleTime: STALE.badges,
    enabled: Boolean(addr),
  });
}

export function useUserPhotos(address?: string | null) {
  const addr = address ?? resolveSelfAddress();
  return useQuery({
    queryKey: qk.photos(addr),
    queryFn: ({ signal }) => fetchUserPhotos(addr, { signal }),
    staleTime: STALE.photos,
    enabled: Boolean(addr),
    retry: false,
  });
}

function emptyProfileVM(addr: string) {
  const hex = (addr || "").replace(/^0x/i, "");
  return {
    address: addr || "",
    name: "",
    tag: hex ? `#${hex.slice(-4)}` : "",
    hasClaimedName: false,
    nameColor: "#FF8362",
    mutualCount: 0,
    bio: "",
    accountUrl: siteUrl("/shop"),
    info: [] as unknown[],
    links: [] as unknown[],
    equipped: [] as unknown[],
  };
}

export function usePassport(address?: string | null) {
  const addr = address ?? resolveSelfAddress();

  const profileQ = useProfile(addr);
  const badgesQ = useUserBadges(addr);
  const photosQ = useUserPhotos(addr);

  const liveAvatar = profileQ.data ?? null;
  const liveProfile = liveAvatar ? mapProfile(liveAvatar, addr) : null;
  const profile = liveProfile ?? emptyProfileVM(addr);
  const faceUrl = liveAvatar ? profileFaceUrl(liveAvatar) : null;

  const liveBadges = badgesQ.data;
  const badges = {
    achieved: liveBadges?.achieved ?? [],
    notAchieved: liveBadges?.notAchieved ?? [],
  };

  const livePhotos = photosQ.data;
  const photos = livePhotos ?? [];

  return {
    address: addr,
    profile,
    faceUrl,
    badges,
    photos,
    isLive: Boolean(liveProfile),
    isLoading: profileQ.isLoading,
    isError: profileQ.isError && !liveProfile,
    error: profileQ.error ?? badgesQ.error ?? photosQ.error ?? null,
  };
}

export function prefetchPassport(queryClient: QueryClient, address?: string | null) {
  const addr = address ?? resolveSelfAddress();
  if (!addr) return;
  queryClient.prefetchQuery({
    queryKey: qk.profile(addr),
    queryFn: ({ signal }) => fetchProfile(addr, { signal }),
    staleTime: STALE.profile,
  });
  queryClient.prefetchQuery({
    queryKey: qk.badges(addr),
    queryFn: ({ signal }) => fetchUserBadges(addr, { signal }),
    staleTime: STALE.badges,
    retry: false,
  });
  queryClient.prefetchQuery({
    queryKey: qk.photos(addr),
    queryFn: ({ signal }) => fetchUserPhotos(addr, { signal }),
    staleTime: STALE.photos,
    retry: false,
  });
}
