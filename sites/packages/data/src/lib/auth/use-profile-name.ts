import { useEffect, useState } from "react";

import { catalystBase } from "../catalyst/client";
import { fetchProfile } from "../catalyst/overlay/profile";

export type ProfileIdentity = {
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
  const [identity, setIdentity] = useState<ProfileIdentity>(EMPTY);

  useEffect(() => {
    if (!isConnected || !address) return;
    let cancelled = false;
    void (async () => {
      try {
        const avatar = await fetchProfile(address);
        if (cancelled || !avatar) return;
        const name = avatar.name?.trim() ?? "";
        const avatarUrl = toFaceUrl(avatar.avatar?.snapshots?.face256);
        setIdentity((prev) => {
          const next = {
            name: name || prev.name,
            avatarUrl: avatarUrl || prev.avatarUrl,
          };
          return next.name === prev.name && next.avatarUrl === prev.avatarUrl
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

  return identity;
}

export function useProfileName(
  address: string | null | undefined,
  isConnected: boolean,
): string {
  return useProfileIdentity(address, isConnected).name;
}
