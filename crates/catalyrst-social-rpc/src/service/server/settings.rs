use super::super::domain::{bvis_to_db, pmp_from_db, pmp_to_db, settings_to_proto, sreact_to_db};
use super::super::helpers::{internal_err, invalid_req, normalize, SocialError};
use super::SocialServiceImpl;
use crate::context::Context;
use crate::proto::v2::*;
use dcl_rpc::service_module_definition::ProcedureContext;

impl SocialServiceImpl {
    pub(super) async fn get_social_settings(
        &self,
        context: ProcedureContext<Context>,
    ) -> Result<GetSocialSettingsResponse, SocialError> {
        let me = Self::caller(&context)?;
        let row = match context.server_context.db().get_social_settings(&me).await {
            Ok(r) => r.unwrap_or_default(),
            Err(e) => {
                return Ok(GetSocialSettingsResponse {
                    response: Some(get_social_settings_response::Response::InternalServerError(
                        internal_err(e.to_string()),
                    )),
                })
            }
        };
        Ok(GetSocialSettingsResponse {
            response: Some(get_social_settings_response::Response::Ok(
                get_social_settings_response::Ok {
                    settings: Some(settings_to_proto(&row)),
                },
            )),
        })
    }

    pub(super) async fn upsert_social_settings(
        &self,
        request: UpsertSocialSettingsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<UpsertSocialSettingsResponse, SocialError> {
        let me = Self::caller(&context)?;
        let pmp = request
            .private_messages_privacy
            .map(|_| pmp_to_db(request.private_messages_privacy()));
        let bvis = request
            .blocked_users_messages_visibility
            .map(|_| bvis_to_db(request.blocked_users_messages_visibility()));
        let sreact = request
            .show_situation_reactions
            .map(|_| sreact_to_db(request.show_situation_reactions()));

        match context
            .server_context
            .db()
            .upsert_social_settings(&me, pmp.as_deref(), bvis.as_deref(), sreact.as_deref())
            .await
        {
            Ok(row) => Ok(UpsertSocialSettingsResponse {
                response: Some(upsert_social_settings_response::Response::Ok(
                    settings_to_proto(&row),
                )),
            }),
            Err(e) => Ok(UpsertSocialSettingsResponse {
                response: Some(
                    upsert_social_settings_response::Response::InternalServerError(internal_err(
                        e.to_string(),
                    )),
                ),
            }),
        }
    }

    pub(super) async fn get_private_messages_settings(
        &self,
        request: GetPrivateMessagesSettingsPayload,
        context: ProcedureContext<Context>,
    ) -> Result<GetPrivateMessagesSettingsResponse, SocialError> {
        let me = Self::caller(&context)?;
        let targets: Vec<String> = request.user.iter().map(|u| normalize(&u.address)).collect();

        const MAX_USER_ADDRESSES: usize = 50;
        if targets.len() > MAX_USER_ADDRESSES {
            return Ok(GetPrivateMessagesSettingsResponse {
                response: Some(
                    get_private_messages_settings_response::Response::InvalidRequest(invalid_req(
                        format!("Too many user addresses: {}", targets.len()),
                    )),
                ),
            });
        }
        let rows = match context
            .server_context
            .db()
            .private_messages_settings(&me, &targets)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(GetPrivateMessagesSettingsResponse {
                    response: Some(
                        get_private_messages_settings_response::Response::InternalServerError(
                            internal_err(e.to_string()),
                        ),
                    ),
                })
            }
        };
        let settings = rows
            .into_iter()
            .map(|(addr, privacy, is_friend)| {
                get_private_messages_settings_response::PrivateMessagesSettings {
                    user: Some(User { address: addr }),
                    private_messages_privacy: pmp_from_db(&privacy) as i32,
                    is_friend,
                }
            })
            .collect();
        Ok(GetPrivateMessagesSettingsResponse {
            response: Some(get_private_messages_settings_response::Response::Ok(
                get_private_messages_settings_response::Ok { settings },
            )),
        })
    }
}
