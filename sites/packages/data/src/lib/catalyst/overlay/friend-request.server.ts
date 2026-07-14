import { z } from "zod";

import { NameColorSchema } from "../generated-schemas/communities";

/**
 * These schemas only ever declare the panel's types -- nothing on this surface
 * parses a payload yet, `loadFriendRequest` below builds every row in process.
 * The `.default()`s therefore never ran; they only promised that a missing
 * `online`, `where` or `hasClaimedName` would silently become "offline" /
 * "Offline" / "unverified" the day a wire read is added here. Every field is
 * required so that day starts with a parse that can fail.
 */
const ColorSchema = NameColorSchema;

const FriendProfileSchema = z.object({
  address: z.string(),
  name: z.string(),
  hasClaimedName: z.boolean(),
  profilePictureUrl: z.string(),
  nameColor: ColorSchema.optional(),
});

const FriendSchema = FriendProfileSchema.extend({
  online: z.boolean(),
  where: z.string(),
});

const RequestSchema = z.object({
  id: z.string(),
  createdAt: z.number(),
  message: z.string().nullable(),
  friend: FriendProfileSchema,
});

const BlockedSchema = FriendProfileSchema.extend({
  blockedAt: z.number().optional(),
});

const CandidateSchema = FriendProfileSchema.extend({
  mutualCount: z.number().optional(),
  friendshipStatus: z.string().optional(),
});

const SelfSchema = z.object({
  address: z.string(),
  name: z.string(),
  hasClaimedName: z.boolean(),
  isGuest: z.boolean(),
});

export type Friend = z.infer<typeof FriendSchema>;
export type FriendRequest = z.infer<typeof RequestSchema>;
export type BlockedUser = z.infer<typeof BlockedSchema>;
export type FriendCandidate = z.infer<typeof CandidateSchema>;
export type FriendSelf = z.infer<typeof SelfSchema>;

export type FriendRequestData = {
  self: FriendSelf;
  friends: Friend[];
  received: FriendRequest[];
  sent: FriendRequest[];
  blocked: BlockedUser[];
  candidate: FriendCandidate;
  counts: { friends: number; received: number; sent: number; blocked: number };
  fallback?: boolean;
};

const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";

function emptyCandidate(address?: string): FriendCandidate {
  return {
    address: address ?? ZERO_ADDRESS,
    name: "user",
    hasClaimedName: false,
    profilePictureUrl: "",
    mutualCount: 0,
    friendshipStatus: "none",
  };
}

export function fixtureCandidateAddress(): string {
  return ZERO_ADDRESS;
}

export function loadFriendRequest(address?: string): FriendRequestData {
  return {
    self: { address: "0x0", name: "you", hasClaimedName: false, isGuest: false },
    friends: [],
    received: [],
    sent: [],
    blocked: [],
    candidate: emptyCandidate(address),
    counts: { friends: 0, received: 0, sent: 0, blocked: 0 },
    fallback: true,
  };
}
