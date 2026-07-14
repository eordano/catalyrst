// GENERATED from catalyrst/ui3/src/generated/catalyst/communities by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { ActiveCommunityVoiceChat } from "@ui/generated/catalyst/communities/ActiveCommunityVoiceChat";
import type { ActiveVoiceChatsData } from "@ui/generated/catalyst/communities/ActiveVoiceChatsData";
import type { CommunityDetail } from "@ui/generated/catalyst/communities/CommunityDetail";
import type { CommunityListItem } from "@ui/generated/catalyst/communities/CommunityListItem";
import type { CommunityMember } from "@ui/generated/catalyst/communities/CommunityMember";
import type { CommunityMemberV2Wire } from "@ui/generated/catalyst/communities/CommunityMemberV2Wire";
import type { CommunityMemberWire } from "@ui/generated/catalyst/communities/CommunityMemberWire";
import type { CreateReferralBody } from "@ui/generated/catalyst/communities/CreateReferralBody";
import type { DirectMessage } from "@ui/generated/catalyst/communities/DirectMessage";
import type { FriendsResponse } from "@ui/generated/catalyst/communities/FriendsResponse";
import type { FriendSummary } from "@ui/generated/catalyst/communities/FriendSummary";
import type { MessagesResponse } from "@ui/generated/catalyst/communities/MessagesResponse";
import type { Mute } from "@ui/generated/catalyst/communities/Mute";
import type { MuteBody } from "@ui/generated/catalyst/communities/MuteBody";
import type { NameColor } from "@ui/generated/catalyst/communities/NameColor";
import type { ReferralProgressStats } from "@ui/generated/catalyst/communities/ReferralProgressStats";
import type { ReferralRewardImage } from "@ui/generated/catalyst/communities/ReferralRewardImage";
import type { SendMessageResponse } from "@ui/generated/catalyst/communities/SendMessageResponse";
import type { VoiceChatStatus } from "@ui/generated/catalyst/communities/VoiceChatStatus";

export const ActiveCommunityVoiceChatSchema = z.object({
  communityId: z.string(),
  communityName: z.string(),
  communityImage: z.string().nullable(),
  isMember: z.boolean(),
  positions: z.array(z.string()),
  worlds: z.array(z.string()),
  participantCount: z.number(),
  moderatorCount: z.number(),
});

export const ActiveVoiceChatsDataSchema = z.object({
  activeChats: z.array(ActiveCommunityVoiceChatSchema),
  total: z.number(),
});

export const VoiceChatStatusSchema = z.object({
  isActive: z.boolean(),
  participantCount: z.number(),
  moderatorCount: z.number(),
});

export const CommunityDetailSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  ownerAddress: z.string(),
  privacy: z.string(),
  active: z.boolean(),
  unlisted: z.boolean(),
  membersCount: z.number(),
  createdAt: z.string(),
  updatedAt: z.string(),
  isLive: z.boolean(),
  voiceChatStatus: VoiceChatStatusSchema,
  visibility: z.string().optional(),
  role: z.string().optional(),
  isBanned: z.boolean().optional(),
  thumbnailUrl: z.string().optional(),
  ownerName: z.string().optional(),
});

export const CommunityListItemSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  ownerAddress: z.string(),
  privacy: z.string(),
  active: z.boolean(),
  unlisted: z.boolean(),
  membersCount: z.number(),
  createdAt: z.string(),
  isLive: z.boolean(),
  voiceChatStatus: VoiceChatStatusSchema,
  visibility: z.string().optional(),
  role: z.string().optional(),
  isBanned: z.boolean().optional(),
  thumbnailUrl: z.string().optional(),
  ownerName: z.string().optional(),
  friends: z.array(z.unknown()).optional(),
});

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

export const CreateReferralBodySchema = z.object({
  referrer: z.string(),
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

export const MuteSchema = z.object({
  address: z.string(),
  muted_at: z.string(),
});

export const MuteBodySchema = z.object({
  muted_address: z.string(),
});

export const ReferralRewardImageSchema = z.object({
  tier: z.number(),
  url: z.string(),
});

export const ReferralProgressStatsSchema = z.object({
  invitedUsersAccepted: z.number(),
  invitedUsersAcceptedViewed: z.number(),
  rewardImages: z.array(ReferralRewardImageSchema),
});

export const SendMessageResponseSchema = z.object({
  message: DirectMessageSchema,
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertActiveCommunityVoiceChat = Assert<Mutual<ActiveCommunityVoiceChat, z.infer<typeof ActiveCommunityVoiceChatSchema>>>;
export type _AssertActiveVoiceChatsData = Assert<Mutual<ActiveVoiceChatsData, z.infer<typeof ActiveVoiceChatsDataSchema>>>;
export type _AssertCommunityDetail = Assert<Mutual<CommunityDetail, z.infer<typeof CommunityDetailSchema>>>;
export type _AssertCommunityListItem = Assert<Mutual<CommunityListItem, z.infer<typeof CommunityListItemSchema>>>;
export type _AssertCommunityMember = Assert<Mutual<CommunityMember, z.infer<typeof CommunityMemberSchema>>>;
export type _AssertCommunityMemberV2Wire = Assert<Mutual<CommunityMemberV2Wire, z.infer<typeof CommunityMemberV2WireSchema>>>;
export type _AssertCommunityMemberWire = Assert<Mutual<CommunityMemberWire, z.infer<typeof CommunityMemberWireSchema>>>;
export type _AssertCreateReferralBody = Assert<Mutual<CreateReferralBody, z.infer<typeof CreateReferralBodySchema>>>;
export type _AssertDirectMessage = Assert<Mutual<DirectMessage, z.infer<typeof DirectMessageSchema>>>;
export type _AssertFriendsResponse = Assert<Mutual<FriendsResponse, z.infer<typeof FriendsResponseSchema>>>;
export type _AssertFriendSummary = Assert<Mutual<FriendSummary, z.infer<typeof FriendSummarySchema>>>;
export type _AssertMessagesResponse = Assert<Mutual<MessagesResponse, z.infer<typeof MessagesResponseSchema>>>;
export type _AssertMute = Assert<Mutual<Mute, z.infer<typeof MuteSchema>>>;
export type _AssertMuteBody = Assert<Mutual<MuteBody, z.infer<typeof MuteBodySchema>>>;
export type _AssertNameColor = Assert<Mutual<NameColor, z.infer<typeof NameColorSchema>>>;
export type _AssertReferralProgressStats = Assert<Mutual<ReferralProgressStats, z.infer<typeof ReferralProgressStatsSchema>>>;
export type _AssertReferralRewardImage = Assert<Mutual<ReferralRewardImage, z.infer<typeof ReferralRewardImageSchema>>>;
export type _AssertSendMessageResponse = Assert<Mutual<SendMessageResponse, z.infer<typeof SendMessageResponseSchema>>>;
export type _AssertVoiceChatStatus = Assert<Mutual<VoiceChatStatus, z.infer<typeof VoiceChatStatusSchema>>>;
