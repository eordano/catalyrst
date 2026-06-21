use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use catalyrst_fed::RateLimitDecision;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::rest::auth_chain::require_signer;
use crate::rest::handlers::friendship::{friendship_status, friendship_statuses};
use crate::rest::http::{ApiError, HttpError};
use crate::rest::AppState;

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct FriendSummary {
    pub address: String,
    pub name: Option<String>,
    #[serde(rename = "hasClaimedName")]
    pub has_claimed_name: bool,
    #[serde(rename = "avatarUrl")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct FriendsResponse {
    pub friends: Vec<FriendSummary>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct DirectMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub body: String,
    #[serde(rename = "sentAt")]
    pub sent_at: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct MessagesResponse {
    pub messages: Vec<DirectMessage>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: i64,
}

#[derive(Debug, Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "communities/")
)]
pub struct SendMessageResponse {
    pub message: DirectMessage,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageBody {
    #[serde(default)]
    pub body: String,
}

const FRIENDS_CAP: i64 = 200;
const MAX_MESSAGE_LEN: usize = 1000;

fn social_pool(state: &AppState) -> Result<&PgPool, ApiError> {
    state
        .mutes_pool
        .as_ref()
        .ok_or_else(|| ApiError::Http(HttpError::new(503, "friends unavailable")))
}

fn is_valid_eth_address(addr: &str) -> bool {
    addr.len() == 42 && addr.starts_with("0x") && addr[2..].chars().all(|c| c.is_ascii_hexdigit())
}

fn iso8601(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Millis, true)
}

async fn require_accepted_friend(social: &PgPool, me: &str, peer: &str) -> Result<(), ApiError> {
    let statuses = friendship_statuses(social, me, std::slice::from_ref(&peer.to_string())).await;
    let accepted = statuses.get(peer).copied() == Some(friendship_status::ACCEPTED);
    if accepted {
        Ok(())
    } else {
        Err(ApiError::Http(HttpError::new(403, "not accepted friends")))
    }
}

fn to_direct_message(
    id: i64,
    sender: String,
    recipient: String,
    body: String,
    created_at: DateTime<Utc>,
) -> DirectMessage {
    DirectMessage {
        id: id.to_string(),
        from: sender,
        to: recipient,
        body,
        sent_at: iso8601(created_at),
    }
}

pub async fn list_friends(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let signer = require_signer(&headers, "get", "/v1/friends")
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let social = social_pool(&state)?;
    let me = signer.to_lowercase();

    let peers: Vec<(String,)> = sqlx::query_as(
        "SELECT peer FROM ( \
           SELECT DISTINCT ON (f.id) \
             CASE WHEN LOWER(f.address_requester) = $1 \
                    THEN LOWER(f.address_requested) \
                    ELSE LOWER(f.address_requester) END AS peer, \
             fa.action AS action \
           FROM friendships f \
           INNER JOIN friendship_actions fa ON fa.friendship_id = f.id \
           WHERE f.is_active = TRUE \
             AND (LOWER(f.address_requester) = $1 OR LOWER(f.address_requested) = $1) \
           ORDER BY f.id, fa.timestamp DESC \
         ) latest \
         WHERE latest.action = 'accept' \
         ORDER BY peer \
         LIMIT $2",
    )
    .bind(&me)
    .bind(FRIENDS_CAP)
    .fetch_all(social)
    .await?;

    let addresses: Vec<String> = peers.into_iter().map(|(p,)| p).collect();
    let profiles = state.profiles.get_profiles(&addresses).await;

    let mut friends: Vec<FriendSummary> = addresses
        .into_iter()
        .map(|address| {
            let info = profiles.get(&address);
            FriendSummary {
                name: info.map(|i| i.name.clone()),
                has_claimed_name: info.map(|i| i.has_claimed_name).unwrap_or(false),
                avatar_url: info.map(|i| i.profile_picture_url.clone()),
                address,
            }
        })
        .collect();

    friends.sort_by(|a, b| {
        let a_key = (
            a.name.is_none(),
            a.name.clone().unwrap_or_default().to_lowercase(),
            a.address.clone(),
        );
        let b_key = (
            b.name.is_none(),
            b.name.clone().unwrap_or_default().to_lowercase(),
            b.address.clone(),
        );
        a_key.cmp(&b_key)
    });

    let total = friends.len() as i64;
    Ok(Json(FriendsResponse { friends, total }))
}

pub async fn get_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(peer): Path<String>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<impl IntoResponse, ApiError> {
    let path = format!("/v1/friends/{}/messages", peer);
    let signer = require_signer(&headers, "get", &path)
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let social = social_pool(&state)?;
    let me = signer.to_lowercase();
    let peer = peer.to_lowercase();
    if !is_valid_eth_address(&peer) {
        return Err(ApiError::bad_request("invalid peer address"));
    }
    require_accepted_friend(social, &me, &peer).await?;

    let mut limit: i64 = 50;
    let mut before: Option<DateTime<Utc>> = None;
    for (k, v) in &pairs {
        match k.as_str() {
            "limit" => {
                if let Ok(n) = v.parse::<i64>() {
                    if n > 0 {
                        limit = n.min(100);
                    }
                }
            }
            "before" => {
                let parsed = DateTime::parse_from_rfc3339(v)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|_| ApiError::bad_request("invalid before timestamp"))?;
                before = Some(parsed);
            }
            _ => {}
        }
    }

    let store = &state.pool;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM friend_messages \
         WHERE (sender_address = $1 AND recipient_address = $2) \
            OR (sender_address = $2 AND recipient_address = $1)",
    )
    .bind(&me)
    .bind(&peer)
    .fetch_one(store)
    .await?;

    let rows: Vec<(i64, String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, sender_address, recipient_address, body, created_at \
         FROM friend_messages \
         WHERE ((sender_address = $1 AND recipient_address = $2) \
             OR (sender_address = $2 AND recipient_address = $1)) \
           AND ($3::timestamptz IS NULL OR created_at < $3) \
         ORDER BY created_at DESC, id DESC \
         LIMIT $4",
    )
    .bind(&me)
    .bind(&peer)
    .bind(before)
    .bind(limit)
    .fetch_all(store)
    .await?;

    let mut messages: Vec<DirectMessage> = rows
        .into_iter()
        .map(|(id, sender, recipient, body, created_at)| {
            to_direct_message(id, sender, recipient, body, created_at)
        })
        .collect();
    messages.reverse();

    Ok(Json(MessagesResponse { messages, total }))
}

pub async fn send_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(peer): Path<String>,
    Json(body): Json<SendMessageBody>,
) -> Result<impl IntoResponse, ApiError> {
    let path = format!("/v1/friends/{}/messages", peer);
    let signer = require_signer(&headers, "post", &path)
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let social = social_pool(&state)?;
    let me = signer.to_lowercase();
    let peer = peer.to_lowercase();
    if !is_valid_eth_address(&peer) {
        return Err(ApiError::bad_request("invalid peer address"));
    }

    if matches!(state.limiter.check(&me), RateLimitDecision::Deny) {
        return Err(ApiError::Http(HttpError::new(429, "rate limit exceeded")));
    }

    let text = body.body.trim();
    if text.is_empty() {
        return Err(ApiError::bad_request("message body is empty"));
    }
    if text.chars().count() > MAX_MESSAGE_LEN {
        return Err(ApiError::bad_request("message body too long"));
    }

    require_accepted_friend(social, &me, &peer).await?;

    let store = &state.pool;
    let (id, created_at): (i64, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO friend_messages (sender_address, recipient_address, body) \
         VALUES ($1, $2, $3) RETURNING id, created_at",
    )
    .bind(&me)
    .bind(&peer)
    .bind(text)
    .fetch_one(store)
    .await?;

    let message = to_direct_message(id, me, peer, text.to_string(), created_at);
    Ok(Json(SendMessageResponse { message }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn accepted_for(peer: &str, statuses: &HashMap<String, i32>) -> bool {
        statuses.get(peer).copied() == Some(friendship_status::ACCEPTED)
    }

    #[test]
    fn accepted_gate_true_only_for_accepted_status() {
        let mut statuses = HashMap::new();
        statuses.insert("0xpeer".to_string(), friendship_status::ACCEPTED);
        assert!(accepted_for("0xpeer", &statuses));

        statuses.insert("0xpeer".to_string(), friendship_status::REQUEST_SENT);
        assert!(!accepted_for("0xpeer", &statuses));

        statuses.insert("0xpeer".to_string(), friendship_status::DELETED);
        assert!(!accepted_for("0xpeer", &statuses));

        statuses.insert("0xpeer".to_string(), friendship_status::BLOCKED);
        assert!(!accepted_for("0xpeer", &statuses));
    }

    #[test]
    fn accepted_gate_false_when_absent() {
        let statuses: HashMap<String, i32> = HashMap::new();
        assert!(!accepted_for("0xpeer", &statuses));
    }

    fn validate_body(raw: &str) -> Result<String, &'static str> {
        let text = raw.trim();
        if text.is_empty() {
            return Err("message body is empty");
        }
        if text.chars().count() > MAX_MESSAGE_LEN {
            return Err("message body too long");
        }
        Ok(text.to_string())
    }

    #[test]
    fn body_validation_rejects_empty_and_whitespace() {
        assert_eq!(validate_body(""), Err("message body is empty"));
        assert_eq!(validate_body("   \n\t "), Err("message body is empty"));
    }

    #[test]
    fn body_validation_trims_then_accepts() {
        assert_eq!(validate_body("  hi there  "), Ok("hi there".to_string()));
    }

    #[test]
    fn body_validation_enforces_max_length_after_trim() {
        let ok = "a".repeat(MAX_MESSAGE_LEN);
        assert_eq!(validate_body(&ok), Ok(ok.clone()));

        let padded = format!("  {}  ", ok);
        assert_eq!(validate_body(&padded), Ok(ok));

        let too_long = "a".repeat(MAX_MESSAGE_LEN + 1);
        assert_eq!(validate_body(&too_long), Err("message body too long"));
    }

    #[test]
    fn eth_address_validation() {
        assert!(is_valid_eth_address(
            "0x1234567890abcdef1234567890abcdef12345678"
        ));
        assert!(!is_valid_eth_address("0x1234"));
        assert!(!is_valid_eth_address(
            "1234567890abcdef1234567890abcdef12345678"
        ));
        assert!(!is_valid_eth_address(
            "0xZZ34567890abcdef1234567890abcdef12345678"
        ));
    }
}
