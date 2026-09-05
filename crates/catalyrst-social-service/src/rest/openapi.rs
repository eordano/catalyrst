use axum::Router;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::rest::handlers;
use crate::rest::AppState;

#[derive(OpenApi)]
#[openapi(info(
    title = "catalyrst-social-service",
    description = "community REST surface"
))]
struct ApiDoc;

pub fn api_router_with_spec() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(
            handlers::communities::get_communities,
            handlers::writes::create_community
        ))
        .routes(routes!(
            handlers::mutes::get_mutes,
            handlers::mutes::add_mute,
            handlers::mutes::remove_mute
        ))
        .routes(routes!(handlers::friends::list_friends))
        .routes(routes!(
            handlers::friends::get_messages,
            handlers::friends::send_message
        ))
        .routes(routes!(handlers::communities::get_raw_thumbnail))
        .routes(routes!(
            handlers::communities::get_community,
            handlers::writes::update_community,
            handlers::writes::update_community_partially,
            handlers::writes::delete_community
        ))
        .routes(routes!(
            handlers::members::get_members,
            handlers::writes::add_member
        ))
        .routes(routes!(
            handlers::writes::remove_member,
            handlers::writes::update_member_role
        ))
        .routes(routes!(handlers::bans::get_bans))
        .routes(routes!(
            handlers::writes::ban_member,
            handlers::writes::unban_member
        ))
        .routes(routes!(
            handlers::places::get_places,
            handlers::writes::add_places
        ))
        .routes(routes!(handlers::writes::remove_place))
        .routes(routes!(
            handlers::posts::get_posts,
            handlers::writes::create_post
        ))
        .routes(routes!(handlers::writes::delete_post))
        .routes(routes!(
            handlers::writes::like_post,
            handlers::writes::unlike_post
        ))
        .routes(routes!(
            handlers::requests::get_community_requests,
            handlers::writes::create_request
        ))
        .routes(routes!(handlers::writes::update_request_status))
        .routes(routes!(handlers::members::get_managed_communities))
        .routes(routes!(
            handlers::members::get_member_communities,
            handlers::writes::member_communities_by_ids
        ))
        .routes(routes!(handlers::requests::get_member_requests))
        .routes(routes!(handlers::invites::get_invites))
        .routes(routes!(handlers::voice::get_active_voice_chats))
        .routes(routes!(
            handlers::referral::get_referral_progress,
            handlers::referral::create_referral,
            handlers::referral::update_referral_signed_up
        ))
        .routes(routes!(handlers::moderation::get_moderation_communities))
        .routes(routes!(handlers::communities::get_communities_v2))
        .routes(routes!(handlers::communities::get_community_v2))
        .routes(routes!(handlers::members::get_members_v2))
        .routes(routes!(handlers::bans::get_bans_v2))
        .routes(routes!(handlers::requests::get_community_requests_v2))
        .routes(routes!(handlers::posts::get_posts_v2))
        .routes(routes!(handlers::requests::get_member_requests_v2))
        .routes(routes!(handlers::admin::list_communities))
        .routes(routes!(handlers::admin::suspend_community))
        .routes(routes!(handlers::admin::unsuspend_community))
        .routes(routes!(handlers::federation::snapshot))
        .routes(routes!(handlers::federation::changes))
        .routes(routes!(handlers::content::put_content))
        .routes(routes!(handlers::content::gc_content))
        .routes(routes!(handlers::content::get_content))
        .split_for_parts()
}

#[cfg(test)]
mod openapi_export {
    #[test]
    fn export_bindings_openapi() {
        let spec = super::api_router_with_spec().1;
        let rendered = serde_json::to_string_pretty(&spec).expect("spec serialises");
        catalyrst_contract_gate::assert_usable_spec(
            "communities",
            &serde_json::from_str(&rendered).expect("spec round-trips through JSON"),
        );
        let Ok(dir) = std::env::var("TS_RS_EXPORT_DIR") else {
            return;
        };
        let out = std::path::Path::new(&dir).join("openapi");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("communities.openapi.json"), rendered).unwrap();
    }
}
