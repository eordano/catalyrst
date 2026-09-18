export const COMMUNITY_STATUSES = ["all", "active", "suspended", "inactive"] as const;
export type CommunityStatus = (typeof COMMUNITY_STATUSES)[number];

export type CommunityDecision = "suspend" | "unsuspend";

export type CommunityModerationCard = {
  id: string;
  name: string;
  owner: string;
  ownerName: string | null;
  privacy: "public" | "private";
  active: boolean;
  suspended: boolean | null;
  membersCount: number;
  thumbnail: string;
  flaggedReason: string;
  status: "Active" | "Suspended" | "Inactive" | "Unknown";
  hue: number;
};

export const STATUS_TONE: Partial<Record<CommunityModerationCard["status"], "ok" | "bad">> = {
  Active: "ok",
  Suspended: "bad",
};

export type ModerateCommunitiesStateValue =
  | "authGate"
  | "list"
  | "reviewCommunity"
  | "decision"
  | "submitting"
  | "moderated";

export { truncateAddress } from "../../data/format";
