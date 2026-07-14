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
  /** Null when the source listing reported no suspension state at all. */
  suspended: boolean | null;
  membersCount: number;
  thumbnail: string;
  flaggedReason: string;
  status: "Active" | "Suspended" | "Inactive" | "Unknown";
  hue: number;
};

export const STATUS_CLASS: Record<CommunityModerationCard["status"], string> = {
  Suspended: "cml-status cml-status--suspended",
  Inactive: "cml-status cml-status--inactive",
  Unknown: "cml-status cml-status--unknown",
  Active: "cml-status cml-status--active",
};

export type ModerateCommunitiesStateValue =
  | "authGate"
  | "list"
  | "reviewCommunity"
  | "decision"
  | "submitting"
  | "moderated";

export { truncateAddress } from "../../data/format";
