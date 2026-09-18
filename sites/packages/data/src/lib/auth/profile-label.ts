import { formatUntrustedLabel } from "./untrusted-label";

type ProfileNameSource = {
  name?: string;
  ethAddress?: string;
  userId?: string;
};

export function isProfileOfAddress(
  profile: ProfileNameSource | null | undefined,
  address: string,
): boolean {
  const reported = profile?.ethAddress ?? profile?.userId;
  if (typeof reported !== "string") return false;
  const owner = reported.trim().toLowerCase();
  return owner.length > 0 && owner === address.trim().toLowerCase();
}

export function profileDisplayName(profile: ProfileNameSource | null | undefined): string {
  return formatUntrustedLabel(profile?.name);
}
