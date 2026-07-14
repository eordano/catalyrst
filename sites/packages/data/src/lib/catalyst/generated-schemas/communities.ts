// GENERATED from catalyrst/ui3/src/generated/catalyst/communities by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { CommunityMember } from "@ui/generated/catalyst/communities/CommunityMember";
import type { CommunityMemberV2Wire } from "@ui/generated/catalyst/communities/CommunityMemberV2Wire";
import type { CommunityMemberWire } from "@ui/generated/catalyst/communities/CommunityMemberWire";
import type { DirectMessage } from "@ui/generated/catalyst/communities/DirectMessage";
import type { FriendsResponse } from "@ui/generated/catalyst/communities/FriendsResponse";
import type { FriendSummary } from "@ui/generated/catalyst/communities/FriendSummary";
import type { MessagesResponse } from "@ui/generated/catalyst/communities/MessagesResponse";
import type { NameColor } from "@ui/generated/catalyst/communities/NameColor";
import type { SendMessageResponse } from "@ui/generated/catalyst/communities/SendMessageResponse";

export const CommunityMemberSchema = z.object({
  communityId: z.string(),
  memberAddress: z.string(),
  role: z.string(),
  joinedAt: z.string(),
});

export const CommunityMemberV2WireSchema = z.object({
  friendshipStatus: z.number(),
  communityId: z.string(),
  memberAddress: z.string(),
  role: z.string(),
  joinedAt: z.string(),
});

export const NameColorSchema = z.object({
  r: z.number(),
  g: z.number(),
  b: z.number(),
});

export const CommunityMemberWireSchema = z.object({
  name: z.string(),
  profilePictureUrl: z.string(),
  hasClaimedName: z.boolean(),
  nameColor: NameColorSchema.optional(),
  friendshipStatus: z.number(),
  communityId: z.string(),
  memberAddress: z.string(),
  role: z.string(),
  joinedAt: z.string(),
});

export const DirectMessageSchema = z.object({
  id: z.string(),
  from: z.string(),
  to: z.string(),
  body: z.string(),
  sentAt: z.string(),
});

export const FriendSummarySchema = z.object({
  address: z.string(),
  name: z.string().nullable(),
  hasClaimedName: z.boolean(),
  avatarUrl: z.string().nullable(),
});

export const FriendsResponseSchema = z.object({
  friends: z.array(FriendSummarySchema),
  total: z.number(),
});

export const MessagesResponseSchema = z.object({
  messages: z.array(DirectMessageSchema),
  total: z.number(),
});

export const SendMessageResponseSchema = z.object({
  message: DirectMessageSchema,
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertCommunityMember = Assert<Mutual<CommunityMember, z.infer<typeof CommunityMemberSchema>>>;
export type _AssertCommunityMemberV2Wire = Assert<Mutual<CommunityMemberV2Wire, z.infer<typeof CommunityMemberV2WireSchema>>>;
export type _AssertCommunityMemberWire = Assert<Mutual<CommunityMemberWire, z.infer<typeof CommunityMemberWireSchema>>>;
export type _AssertDirectMessage = Assert<Mutual<DirectMessage, z.infer<typeof DirectMessageSchema>>>;
export type _AssertFriendsResponse = Assert<Mutual<FriendsResponse, z.infer<typeof FriendsResponseSchema>>>;
export type _AssertFriendSummary = Assert<Mutual<FriendSummary, z.infer<typeof FriendSummarySchema>>>;
export type _AssertMessagesResponse = Assert<Mutual<MessagesResponse, z.infer<typeof MessagesResponseSchema>>>;
export type _AssertNameColor = Assert<Mutual<NameColor, z.infer<typeof NameColorSchema>>>;
export type _AssertSendMessageResponse = Assert<Mutual<SendMessageResponse, z.infer<typeof SendMessageResponseSchema>>>;
