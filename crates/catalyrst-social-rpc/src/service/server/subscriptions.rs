use super::super::helpers::{stream_for, SocialError};
use super::SocialServiceImpl;
use crate::context::Context;
use crate::proto::v2::*;
use crate::pubsub::SocialEvent;
use dcl_rpc::service_module_definition::ProcedureContext;
use dcl_rpc::stream_protocol::Generator;
use tokio::sync::broadcast::error::RecvError;

impl SocialServiceImpl {
    pub(super) async fn subscribe_to_friendship_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<FriendshipUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        Ok(stream_for(
            context.server_context.pubsub(),
            &me,
            |e| match e {
                SocialEvent::Friendship(u) => Some(u),
                _ => None,
            },
        ))
    }

    pub(super) async fn subscribe_to_friend_connectivity_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<FriendConnectivityUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        let ctx = context.server_context.clone();
        let mut rx = ctx.pubsub().subscribe(&me);

        let online_candidates: Vec<String> = {
            let friends = ctx.db().friend_addresses(&me).await.unwrap_or_default();
            friends.into_iter().filter(|f| ctx.is_online(f)).collect()
        };
        let online_now = ctx
            .db()
            .online_friends(&me, &online_candidates)
            .await
            .unwrap_or(online_candidates);
        let snapshot = ctx.profiles().friend_profiles(&online_now).await;

        let (generator, yielder) = Generator::create();
        tokio::spawn(async move {
            for friend in snapshot {
                let update = FriendConnectivityUpdate {
                    friend: Some(friend),
                    status: ConnectivityStatus::Online as i32,
                };
                if yielder.r#yield(update).await.is_err() {
                    return;
                }
            }
            loop {
                match rx.recv().await {
                    Ok(SocialEvent::FriendConnectivity(u)) => {
                        if yielder.r#yield(u).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => continue,
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped,
                            "friend connectivity subscription lagged; events dropped"
                        );
                        continue;
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });
        Ok(generator)
    }

    pub(super) async fn subscribe_to_block_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<BlockUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        Ok(stream_for(
            context.server_context.pubsub(),
            &me,
            |e| match e {
                SocialEvent::Block(u) => Some(u),
                _ => None,
            },
        ))
    }

    pub(super) async fn subscribe_to_community_member_connectivity_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<CommunityMemberConnectivityUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        Ok(stream_for(
            context.server_context.pubsub(),
            &me,
            |e| match e {
                SocialEvent::CommunityMember(u) => Some(u),
                _ => None,
            },
        ))
    }

    pub(super) async fn subscribe_to_private_voice_chat_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<PrivateVoiceChatUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        Ok(stream_for(
            context.server_context.pubsub(),
            &me,
            |e| match e {
                SocialEvent::PrivateVoice(u) => Some(u),
                _ => None,
            },
        ))
    }

    pub(super) async fn subscribe_to_community_voice_chat_updates(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<ServerStreamResponse<CommunityVoiceChatUpdate>, SocialError> {
        let me = Self::caller(&context)?;
        Ok(stream_for(
            context.server_context.pubsub(),
            &me,
            |e| match e {
                SocialEvent::CommunityVoice(u) => Some(u),
                _ => None,
            },
        ))
    }
}
