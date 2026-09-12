
import { z } from "zod";

const nullableStr = z.string().nullish();

export const CommunityMemberSchema = z.object({
  memberAddress: z.string(),
  role: z.string(),
  joinedAt: nullableStr,
  name: z.string(),
  profilePictureUrl: z.string(),
  hasClaimedName: z.boolean(),
});

export type CommunityMemberWire = z.infer<typeof CommunityMemberSchema>;

export const CommunityEventSchema = z.object({
  id: z.string(),
  name: nullableStr,
  image: nullableStr,
  creatorName: nullableStr,
  timeLabel: nullableStr,
});

export type CommunityEventWire = z.infer<typeof CommunityEventSchema>;

export const CommunityPostSchema = z.object({
  id: z.string(),
  authorAddress: z.string(),
  content: z.string(),
  createdAt: nullableStr,
  likesCount: z.number(),
  isLikedByUser: z.boolean(),
  authorName: z.string(),
  authorProfilePictureUrl: z.string(),
  authorHasClaimedName: z.boolean(),
});

export type CommunityPostWire = z.infer<typeof CommunityPostSchema>;

export const CommunityPlaceSchema = z.object({
  id: z.string(),
  addedBy: z.string(),
  addedAt: nullableStr,
});

export type CommunityPlaceWire = z.infer<typeof CommunityPlaceSchema>;

export const CommunitySchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  ownerAddress: z.string(),
  ownerName: nullableStr,
  thumbnailUrl: nullableStr,
  privacy: z.enum(["public", "private"]),
  visibility: z.enum(["all", "unlisted"]).nullish(),
  membersCount: z.number(),
  isLive: z.boolean(),
  role: z.string().nullish(),
});

export type CommunityWire = z.infer<typeof CommunitySchema>;
