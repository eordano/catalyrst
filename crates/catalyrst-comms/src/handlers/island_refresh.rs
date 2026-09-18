use std::sync::{Arc, LazyLock};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Extension, State};
use axum::http::header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use catalyrst_types::AuthLinkType;
use prost::Message;

use crate::assignment_fence::{
    FenceError, RealmAssignmentReader, RealmAssignmentSnapshot, TokenFence,
};
use crate::auth_chain::{extract_auth_chain, verify_signed_fetch, AuthChain};
use crate::cluster_gateway::StateGateway;
use crate::cluster_subscriber::ClusterGateway;
use crate::extract::device_identifier;
use crate::AppState;

pub const ISLAND_REFRESH_PATH: &str = "/island-refresh";
pub const ISLAND_REFRESH_TTL_SECONDS: u64 = 60;
pub const ISLAND_REFRESH_CONTENT_TYPE: &str = "application/x-protobuf";
const ISLAND_REFRESH_DEADLINE: Duration = Duration::from_secs(2);
const MAX_CONCURRENT_ISLAND_REFRESHES: usize = 128;
static ISLAND_REFRESH_PERMITS: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_ISLAND_REFRESHES));

#[derive(Clone, PartialEq, Message)]
pub struct IslandRefreshResponse {
    #[prost(string, tag = "1")]
    pub adapter: String,
    #[prost(uint32, tag = "2")]
    pub expires_in_seconds: u32,
}

pub struct IslandRefreshError(StatusCode);

impl IntoResponse for IslandRefreshError {
    fn into_response(self) -> Response {
        Response::builder()
            .status(self.0)
            .header(CONTENT_TYPE, ISLAND_REFRESH_CONTENT_TYPE)
            .header(CACHE_CONTROL, "no-store")
            .body(Body::empty())
            .expect("static island refresh response")
    }
}

pub async fn refresh(
    State(state): State<AppState>,
    Extension(assignments): Extension<Arc<dyn RealmAssignmentReader>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, IslandRefreshError> {
    require_protobuf_headers(&headers)?;
    if !body.is_empty() {
        return Err(IslandRefreshError(StatusCode::BAD_REQUEST));
    }
    let _permit = ISLAND_REFRESH_PERMITS
        .try_acquire()
        .map_err(|_| IslandRefreshError(StatusCode::SERVICE_UNAVAILABLE))?;
    tokio::time::timeout(
        ISLAND_REFRESH_DEADLINE,
        refresh_bounded(&state, &*assignments, &headers),
    )
    .await
    .map_err(|_| IslandRefreshError(StatusCode::SERVICE_UNAVAILABLE))?
}

async fn refresh_bounded(
    state: &AppState,
    assignments: &dyn RealmAssignmentReader,
    headers: &HeaderMap,
) -> Result<Response, IslandRefreshError> {
    let chain =
        extract_auth_chain(headers).map_err(|_| IslandRefreshError(StatusCode::BAD_REQUEST))?;
    let mut canonical_headers = headers.clone();
    canonical_headers.remove("x-original-path");
    let signed = verify_signed_fetch(&canonical_headers, "post", ISLAND_REFRESH_PATH, &[])
        .await
        .map_err(|error| {
            IslandRefreshError(
                StatusCode::from_u16(error.status).unwrap_or(StatusCode::BAD_REQUEST),
            )
        })?;
    let session =
        effective_session(&chain).map_err(|_| IslandRefreshError(StatusCode::FORBIDDEN))?;
    let wallet = signed.signer.as_str();
    let device_id = device_identifier(&signed.metadata);
    ensure_not_banned(state, wallet, device_id.as_deref()).await?;

    let before = read_current(assignments, wallet, &session).await?;
    let fence = TokenFence {
        audience: before.audience.clone(),
        lane: before.lane.clone(),
        owner_session: before.owner_session.clone(),
        owner_epoch: before.owner_epoch,
        assignment_revision: before.assignment_revision,
        fencing_token: before.fencing_token,
    };
    let adapter = StateGateway::new(state.clone())
        .mint_fenced_connection_string(
            &before.wallet,
            &before.island_id,
            ISLAND_REFRESH_TTL_SECONDS,
            &fence,
        )
        .await
        .map_err(|_| IslandRefreshError(StatusCode::SERVICE_UNAVAILABLE))?;

    let after = read_current(assignments, wallet, &session).await?;
    if after != before {
        return Err(IslandRefreshError(StatusCode::FORBIDDEN));
    }
    ensure_not_banned(state, wallet, device_id.as_deref()).await?;

    let response = IslandRefreshResponse {
        adapter,
        expires_in_seconds: ISLAND_REFRESH_TTL_SECONDS as u32,
    };
    let mut encoded = Vec::with_capacity(response.encoded_len());
    response
        .encode(&mut encoded)
        .map_err(|_| IslandRefreshError(StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, ISLAND_REFRESH_CONTENT_TYPE)
        .header(CACHE_CONTROL, "no-store")
        .body(Body::from(encoded))
        .expect("static island refresh response"))
}

fn effective_session(chain: &AuthChain) -> Result<String, ()> {
    let mut session = None;
    for link in &chain.links {
        if matches!(
            link.kind,
            AuthLinkType::EcdsaEphemeral | AuthLinkType::EcdsaEip1654Ephemeral
        ) {
            if session.is_some() {
                return Err(());
            }
            let (_, address, _) =
                catalyrst_crypto::auth_chain::parse_ephemeral_payload(&link.payload)
                    .map_err(|_| ())?;
            session = Some(address.to_lowercase());
        }
    }
    Ok(session.unwrap_or_else(|| chain.signer.as_str().to_string()))
}

async fn read_current(
    assignments: &dyn RealmAssignmentReader,
    wallet: &str,
    session: &str,
) -> Result<RealmAssignmentSnapshot, IslandRefreshError> {
    assignments
        .current_realm_assignment(wallet, session)
        .await
        .map_err(|error| match error {
            FenceError::Unavailable => IslandRefreshError(StatusCode::SERVICE_UNAVAILABLE),
            FenceError::Conflict | FenceError::Unowned | FenceError::Invalid => {
                IslandRefreshError(StatusCode::FORBIDDEN)
            }
        })
}

async fn ensure_not_banned(
    state: &AppState,
    wallet: &str,
    presented_device_id: Option<&str>,
) -> Result<(), IslandRefreshError> {
    let recorded = crate::access_gate::is_connection_banned(state, wallet, None)
        .await
        .map_err(|_| IslandRefreshError(StatusCode::SERVICE_UNAVAILABLE))?;
    let presented = match presented_device_id {
        Some(device_id) => crate::access_gate::is_connection_banned(state, wallet, Some(device_id))
            .await
            .map_err(|_| IslandRefreshError(StatusCode::SERVICE_UNAVAILABLE))?,
        None => false,
    };
    if recorded || presented {
        return Err(IslandRefreshError(StatusCode::FORBIDDEN));
    }
    Ok(())
}

fn require_protobuf_headers(headers: &HeaderMap) -> Result<(), IslandRefreshError> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if content_type != Some(ISLAND_REFRESH_CONTENT_TYPE) {
        return Err(IslandRefreshError(StatusCode::UNSUPPORTED_MEDIA_TYPE));
    }
    let accepts_protobuf = headers
        .get_all(ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|value| value.split(';').next())
        .map(str::trim)
        .any(|value| value == ISLAND_REFRESH_CONTENT_TYPE);
    accepts_protobuf
        .then_some(())
        .ok_or(IslandRefreshError(StatusCode::NOT_ACCEPTABLE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth_chain::AuthLink;

    #[test]
    fn effective_session_is_the_ephemeral_authority() {
        let chain = AuthChain {
            signer: "0x1111111111111111111111111111111111111111".into(),
            links: vec![
                AuthLink {
                    kind: AuthLinkType::SIGNER,
                    payload: "0x1111111111111111111111111111111111111111".into(),
                    signature: String::new(),
                },
                AuthLink {
                    kind: AuthLinkType::EcdsaEphemeral,
                    payload: "Decentraland Login\nEphemeral address: 0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\nExpiration: 2099-01-01T00:00:00Z".into(),
                    signature: "unused".into(),
                },
                AuthLink {
                    kind: AuthLinkType::EcdsaSignedEntity,
                    payload: "signed fetch payload".into(),
                    signature: "unused".into(),
                },
            ],
        };
        assert_eq!(
            effective_session(&chain).as_deref(),
            Ok("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn wallet_only_chain_uses_the_verified_root_signer() {
        let chain = AuthChain {
            signer: "0x1111111111111111111111111111111111111111".into(),
            links: vec![AuthLink {
                kind: AuthLinkType::SIGNER,
                payload: "0x1111111111111111111111111111111111111111".into(),
                signature: String::new(),
            }],
        };
        assert_eq!(
            effective_session(&chain).as_deref(),
            Ok("0x1111111111111111111111111111111111111111")
        );
    }

    #[test]
    fn multiple_ephemeral_delegations_are_ambiguous_and_rejected() {
        let ephemeral = AuthLink {
            kind: AuthLinkType::EcdsaEphemeral,
            payload: "Decentraland Login\nEphemeral address: 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nExpiration: 2099-01-01T00:00:00Z".into(),
            signature: "unused".into(),
        };
        let chain = AuthChain {
            signer: "0x1111111111111111111111111111111111111111".into(),
            links: vec![
                AuthLink {
                    kind: AuthLinkType::SIGNER,
                    payload: "0x1111111111111111111111111111111111111111".into(),
                    signature: String::new(),
                },
                ephemeral.clone(),
                ephemeral,
            ],
        };
        assert_eq!(effective_session(&chain), Err(()));
    }
}
