use super::helpers::{normalize, SocialError};
use crate::context::Context;
use crate::proto::v2::*;
use async_trait::async_trait;
use dcl_rpc::service_module_definition::ProcedureContext;

mod blocks;
mod community_voice;
mod friends;
mod private_voice;
mod settings;
mod subscriptions;

pub struct SocialServiceImpl;

impl SocialServiceImpl {
    pub(super) fn caller(ctx: &ProcedureContext<Context>) -> Result<String, SocialError> {
        ctx.server_context
            .identity(ctx.transport_id)
            .map(|a| normalize(&a))
            .ok_or(SocialError::Unauthenticated)
    }
}

#[async_trait]
impl SocialServiceServer<Context, SocialError> for SocialServiceImpl {
    async fn get_friends(
        &self,
        request: GetFriendsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PaginatedFriendsProfilesResponse, SocialError> {
        self.get_friends(request, context).await
    }

    async fn get_mutual_friends(
        &self,
        request: GetMutualFriendsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PaginatedFriendsProfilesResponse, SocialError> {
        self.get_mutual_friends(request, context).await
    }

    async fn get_pending_friendship_requests(
        &self,
        request: GetFriendshipRequestsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PaginatedFriendshipRequestsResponse, SocialError> {
        self.get_pending_friendship_requests(request, context).await
    }

    async fn get_sent_friendship_requests(
        &self,
        request: GetFriendshipRequestsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PaginatedFriendshipRequestsResponse, SocialError> {
        self.get_sent_friendship_requests(request, context).await
    }

    async fn get_friendship_status(
        &self,
        request: GetFriendshipStatusPayload,
        context: ProcedureContext<Context>,
    ) -> Result<GetFriendshipStatusResponse, SocialError> {
        self.get_friendship_status(request, context).await
    }

    async fn upsert_friendship(
        &self,
        request: UpsertFriendshipPayload,
        context: ProcedureContext<Context>,
    ) -> Result<UpsertFriendshipResponse, SocialError> {
        self.upsert_friendship(request, context).await
    }

    async fn block_user(
        &self,
        request: BlockUserPayload,
        context: ProcedureContext<Context>,
    ) -> Result<BlockUserResponse, SocialError> {
        self.block_user(request, context).await
    }

    async fn unblock_user(
        &self,
        request: UnblockUserPayload,
        context: ProcedureContext<Context>,
    ) -> Result<UnblockUserResponse, SocialError> {
        self.unblock_user(request, context).await
    }

    async fn get_blocked_users(
        &self,
        request: GetBlockedUsersPayload,
        context: ProcedureContext<Context>,
    ) -> Result<GetBlockedUsersResponse, SocialError> {
        self.get_blocked_users(request, context).await
    }

    async fn get_blocking_status(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<GetBlockingStatusResponse, SocialError> {
        self.get_blocking_status(context).await
    }

    async fn get_social_settings(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<GetSocialSettingsResponse, SocialError> {
        self.get_social_settings(context).await
    }

    async fn upsert_social_settings(
        &self,
        request: UpsertSocialSettingsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<UpsertSocialSettingsResponse, SocialError> {
        self.upsert_social_settings(request, context).await
    }

    async fn get_private_messages_settings(
        &self,
        request: GetPrivateMessagesSettingsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<GetPrivateMessagesSettingsResponse, SocialError> {
        self.get_private_messages_settings(request, context).await
    }

    async fn subscribe_to_friendship_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<FriendshipUpdate>, SocialError> {
        self.subscribe_to_friendship_updates(context).await
    }

    async fn subscribe_to_friend_connectivity_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<FriendConnectivityUpdate>, SocialError> {
        self.subscribe_to_friend_connectivity_updates(context).await
    }

    async fn subscribe_to_block_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<BlockUpdate>, SocialError> {
        self.subscribe_to_block_updates(context).await
    }

    async fn subscribe_to_community_member_connectivity_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<CommunityMemberConnectivityUpdate>, SocialError> {
        self.subscribe_to_community_member_connectivity_updates(context)
            .await
    }

    async fn subscribe_to_private_voice_chat_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<PrivateVoiceChatUpdate>, SocialError> {
        self.subscribe_to_private_voice_chat_updates(context).await
    }

    async fn subscribe_to_community_voice_chat_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<CommunityVoiceChatUpdate>, SocialError> {
        self.subscribe_to_community_voice_chat_updates(context)
            .await
    }

    async fn start_private_voice_chat(
        &self,
        request: StartPrivateVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<StartPrivateVoiceChatResponse, SocialError> {
        self.start_private_voice_chat(request, context).await
    }

    async fn accept_private_voice_chat(
        &self,
        request: AcceptPrivateVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<AcceptPrivateVoiceChatResponse, SocialError> {
        self.accept_private_voice_chat(request, context).await
    }

    async fn reject_private_voice_chat(
        &self,
        request: RejectPrivateVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<RejectPrivateVoiceChatResponse, SocialError> {
        self.reject_private_voice_chat(request, context).await
    }

    async fn end_private_voice_chat(
        &self,
        request: EndPrivateVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<EndPrivateVoiceChatResponse, SocialError> {
        self.end_private_voice_chat(request, context).await
    }

    async fn get_incoming_private_voice_chat_request(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<GetIncomingPrivateVoiceChatRequestResponse, SocialError> {
        self.get_incoming_private_voice_chat_request(context).await
    }

    async fn start_community_voice_chat(
        &self,
        request: StartCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<StartCommunityVoiceChatResponse, SocialError> {
        self.start_community_voice_chat(request, context).await
    }

    async fn join_community_voice_chat(
        &self,
        request: JoinCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<JoinCommunityVoiceChatResponse, SocialError> {
        self.join_community_voice_chat(request, context).await
    }

    async fn request_to_speak_in_community_voice_chat(
        &self,
        request: RequestToSpeakInCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<RequestToSpeakInCommunityVoiceChatResponse, SocialError> {
        self.request_to_speak_in_community_voice_chat(request, context)
            .await
    }

    async fn promote_speaker_in_community_voice_chat(
        &self,
        request: PromoteSpeakerInCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<PromoteSpeakerInCommunityVoiceChatResponse, SocialError> {
        self.promote_speaker_in_community_voice_chat(request, context)
            .await
    }

    async fn demote_speaker_in_community_voice_chat(
        &self,
        request: DemoteSpeakerInCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<DemoteSpeakerInCommunityVoiceChatResponse, SocialError> {
        self.demote_speaker_in_community_voice_chat(request, context)
            .await
    }

    async fn kick_player_from_community_voice_chat(
        &self,
        request: KickPlayerFromCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<KickPlayerFromCommunityVoiceChatResponse, SocialError> {
        self.kick_player_from_community_voice_chat(request, context)
            .await
    }

    async fn reject_speak_request_in_community_voice_chat(
        &self,
        request: RejectSpeakRequestInCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<RejectSpeakRequestInCommunityVoiceChatResponse, SocialError> {
        self.reject_speak_request_in_community_voice_chat(request, context)
            .await
    }

    async fn end_community_voice_chat(
        &self,
        request: EndCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<EndCommunityVoiceChatResponse, SocialError> {
        self.end_community_voice_chat(request, context).await
    }

    async fn mute_speaker_from_community_voice_chat(
        &self,
        request: MuteSpeakerFromCommunityVoiceChatPayload,
        context: ProcedureContext<Context>,
    ) -> Result<MuteSpeakerFromCommunityVoiceChatResponse, SocialError> {
        self.mute_speaker_from_community_voice_chat(request, context)
            .await
    }
}
