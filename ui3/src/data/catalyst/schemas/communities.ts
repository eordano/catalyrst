// Wire shapes for the communities reader.
//
// Schemas and the WIRE types they infer, and nothing else. A perf build aliases
// this whole module to a generated stub (vite.validate.js), which is what lets
// zod leave the bundle -- so a transform put here would run in one build and not
// the other, and the stub would be changing behaviour rather than only changing
// what is checked.
//
// Normalization therefore lives in the reader (../communitiesSchema.ts): the
// cdn.decentraland.org -> federated CDN rewrite, which needs `serviceBase` and
// so cannot be expressed in a shape at all, and every nullish field the exported
// `Community` type promises as null. Those run in both modes.

import { z } from "zod";

const nullableStr = z.string().nullish();

/**
 * `CommunityMemberWire` (catalyrst-social-service) serializes every one of these,
 * so a member row that is missing one is not a member with a blank name.
 */
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

/**
 * `visibility` and `role` describe the *viewer's* relationship to the community
 * and are emitted only on a signed request; absent therefore means "nobody
 * asked", which is what an anonymous read actually knows, and the reader's
 * normalizer states that as null. The rest is emitted always.
 */
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
