pub mod admin;
pub mod bans;
pub mod client;
pub mod communities;
pub mod content;
pub mod dto;
pub mod enrich;
pub mod error;
pub mod federation;
pub mod friends;
pub mod friendship;
pub mod invites;
pub mod members;
pub mod moderation;
pub mod mutes;
pub mod permissions;
pub mod ping;
pub mod places;
pub mod posts;
pub mod referral;
pub mod requests;
pub mod voice;
pub mod writes;

use uuid::Uuid;

use crate::rest::handlers::error::CommError;
use crate::rest::AppState;

pub(crate) async fn require_membership_of_a_private_community(
    state: &AppState,
    id: Uuid,
    signer: Option<&catalyrst_crypto::Signer>,
    refusal_message: impl FnOnce(&str) -> String,
) -> Result<(), CommError> {
    let Some(addr) = signer.map(catalyrst_crypto::Signer::as_str) else {
        return Ok(());
    };
    if !state.communities.is_private(id).await? {
        return Ok(());
    }
    let standing =
        crate::rest::community_membership_authority::load_standing_from_community_members(
            &state.pool,
            id,
            addr,
        )
        .await?;
    if standing.a_membership_row_exists_for_this_wallet() {
        return Ok(());
    }
    Err(CommError::not_authorized(refusal_message(addr)))
}
