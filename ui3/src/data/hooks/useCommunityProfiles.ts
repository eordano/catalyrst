import { useQuery } from "@tanstack/react-query";
import type { CommunityMember } from "../catalyst/communities";
import { sendJSON, type RequestOpts } from "../catalyst/client";
import { parseProfileEnvelope, profileFaceUrl } from "../catalyst/profile";
import { asAddress } from "../catalyst/sceneOwner";
import { STALE } from "../queryKeys";

type MemberProfile = { name: string; picture: string | null; claimed: boolean | null };
const memberName = (name: string) => asAddress(name) ? "" : name.trim();

export async function fetchCommunityProfiles(addresses: string[], opts: RequestOpts = {}) {
  const profiles: Record<string, MemberProfile> = {};
  for (let i = 0; i < addresses.length; i += 100) {
    const batch = await sendJSON<unknown[]>("/lambdas/profiles", { ...opts, body: { ids: addresses.slice(i, i + 100) } });
    for (const envelope of batch ?? []) {
      for (const avatar of parseProfileEnvelope(envelope).avatars) {
        const address = asAddress(avatar.ethAddress) ?? asAddress(avatar.userId);
        if (address) profiles[address] = { name: memberName(avatar.name ?? ""), picture: profileFaceUrl(avatar), claimed: avatar.hasClaimedName };
      }
    }
  }
  return profiles;
}

export function useCommunityProfiles(members: CommunityMember[], enabled: boolean) {
  const addresses = [...new Set(members.filter(member => !memberName(member.name) || !member.profilePictureUrl)
    .map(member => asAddress(member.memberAddress)).filter((address): address is string => !!address))].sort();
  const profiles = useQuery({
    queryKey: ["community-member-profiles", addresses],
    queryFn: ({ signal }) => fetchCommunityProfiles(addresses, { signal }),
    enabled: enabled && addresses.length > 0,
    staleTime: STALE.profile,
    retry: false,
  });
  return members.map(member => {
    const profile = profiles.data?.[member.memberAddress.toLowerCase()];
    return { ...member, name: memberName(member.name) || profile?.name || "",
      profilePictureUrl: member.profilePictureUrl || profile?.picture || "",
      hasClaimedName: profile?.claimed ?? member.hasClaimedName };
  });
}
