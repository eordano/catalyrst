import { useEffect, useState } from "react";

import { catalystBase } from "../catalyst/client";
import { fetchProfile } from "../catalyst/overlay/profile";
import { isProfileOfAddress, profileDisplayName } from "./profile-label";

type ProfileIdentity = {
  name: string;
  avatarUrl: string;
};

const EMPTY: ProfileIdentity = { name: "", avatarUrl: "" };

function toFaceUrl(face: string | undefined): string {
  if (!face) return "";
  if (/^https?:\/\//i.test(face) || face.startsWith("data:")) return face;
  return `${catalystBase()}/content/contents/${face}`;
}

export function useProfileIdentity(
  address: string | null | undefined,
  isConnected: boolean,
): ProfileIdentity {
  const [identity, setIdentity] = useState<ProfileIdentity & { address: string }>({ ...EMPTY, address: "" });

  useEffect(() => {
    if (!isConnected || !address) return;
    let cancelled = false;
    void (async () => {
      try {
        const avatar = await fetchProfile(address);
        if (cancelled || !avatar || !isProfileOfAddress(avatar, address)) return;
        const name = profileDisplayName(avatar);
        const avatarUrl = toFaceUrl(avatar.avatar?.snapshots?.face256);
        setIdentity((prev) => {
          const next = {
            address: address.toLowerCase(),
            name,
            avatarUrl,
          };
          return next.address === prev.address && next.name === prev.name && next.avatarUrl === prev.avatarUrl
            ? prev
            : next;
        });
      } catch {
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isConnected, address]);

  return isConnected && address?.toLowerCase() === identity.address ? identity : EMPTY;
}

export function useProfileName(
  address: string | null | undefined,
  isConnected: boolean,
): string {
  return useProfileIdentity(address, isConnected).name;
}
