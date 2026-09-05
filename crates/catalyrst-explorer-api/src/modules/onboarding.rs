use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::modules::{json_response, ErrorMessage};
use crate::AppState;

const SEQUENCE_HOURS: [i64; 4] = [0, 12, 24, 36];
const MIN_CHECKPOINT: i64 = 1;
const MAX_CHECKPOINT: i64 = 7;
const MAX_USER_IDENTIFIER_LEN: usize = 255;
const MAX_EMAIL_LEN: usize = 255;
const MAX_WALLET_LEN: usize = 255;
const MAX_SOURCE_LEN: usize = 50;

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct CheckpointRow {
    user_id: String,
    id_type: String,
    email: Option<String>,
    wallet: Option<String>,
    checkpoint: i64,
    reached_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    source: Option<String>,
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct CheckpointPayload {
    user_identifier: String,
    identifier_type: String,
    checkpoint_id: i64,
    action: String,
    email: Option<String>,
    wallet: Option<String>,
    source: Option<String>,
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingNudge {
    user_id: String,
    checkpoint_id: i64,
    email: String,
}

#[derive(Serialize)]
struct CheckpointAck {
    success: bool,
}

#[derive(Serialize)]
struct PendingNudgesResponse {
    sequence: i64,
    count: u64,
    nudges: Vec<PendingNudge>,
}

#[derive(Default)]
struct OnboardingStore {
    checkpoints: Vec<CheckpointRow>,
    sent_nudges: std::collections::HashSet<(String, i64, i64)>,
}

impl OnboardingStore {
    fn resolve_wallet_identity(&self, wallet_address: &str) -> Option<(String, String)> {
        self.checkpoints
            .iter()
            .filter(|r| r.wallet.as_deref() == Some(wallet_address) && r.email.is_some())
            .min_by_key(|r| r.checkpoint)
            .map(|r| (r.user_id.clone(), r.email.clone().unwrap_or_default()))
    }

    fn record_checkpoint(&mut self, payload: CheckpointPayload, now: DateTime<Utc>) {
        let CheckpointPayload {
            mut user_identifier,
            mut identifier_type,
            checkpoint_id,
            action,
            mut email,
            mut wallet,
            source,
            metadata,
        } = payload;

        if let Some(w) = wallet.as_mut() {
            *w = w.to_lowercase();
        }

        if identifier_type == "wallet" && email.is_none() {
            let lowered = user_identifier.to_lowercase();
            if let Some((resolved_user, resolved_email)) = self.resolve_wallet_identity(&lowered) {
                wallet = Some(wallet.unwrap_or(lowered));
                user_identifier = resolved_user;
                identifier_type = "email".to_string();
                email = Some(resolved_email);
            } else {
                wallet = Some(wallet.unwrap_or(lowered));
            }
        }

        if action == "completed" {
            if let Some(row) = self.checkpoints.iter_mut().find(|r| {
                r.user_id == user_identifier
                    && r.checkpoint == checkpoint_id
                    && r.completed_at.is_none()
            }) {
                row.completed_at = Some(now);
                if email.is_some() {
                    row.email = email.clone();
                }
                if wallet.is_some() {
                    row.wallet = wallet.clone();
                }
            }
            return;
        }

        if let Some(row) = self
            .checkpoints
            .iter_mut()
            .find(|r| r.user_id == user_identifier && r.checkpoint == checkpoint_id)
        {
            if email.is_some() {
                row.email = email.clone();
            }
            if wallet.is_some() {
                row.wallet = wallet.clone();
            }
            if metadata.is_some() {
                row.metadata = metadata.clone();
            }
        } else {
            self.checkpoints.push(CheckpointRow {
                user_id: user_identifier.clone(),
                id_type: identifier_type.clone(),
                email: email.clone(),
                wallet: wallet.clone(),
                checkpoint: checkpoint_id,
                reached_at: now,
                completed_at: None,
                source: source.clone(),
                metadata: metadata.clone(),
            });
        }

        if checkpoint_id > 1 {
            if let Some(row) = self.checkpoints.iter_mut().find(|r| {
                r.user_id == user_identifier
                    && r.checkpoint == checkpoint_id - 1
                    && r.completed_at.is_none()
            }) {
                row.completed_at = Some(now);
            }
        }
    }

    fn pending_nudges(&self, sequence: i64, now: DateTime<Utc>) -> Vec<PendingNudge> {
        let hours = SEQUENCE_HOURS[sequence as usize];
        let threshold = now - Duration::hours(hours);

        let mut max_cp: std::collections::HashMap<&str, i64> =
            std::collections::HashMap::with_capacity(self.checkpoints.len());
        for r in &self.checkpoints {
            max_cp
                .entry(r.user_id.as_str())
                .and_modify(|m| *m = (*m).max(r.checkpoint))
                .or_insert(r.checkpoint);
        }

        self.checkpoints
            .iter()
            .filter_map(|oc| {
                let email = oc.email.as_ref()?;
                if oc.completed_at.is_some() {
                    return None;
                }
                if oc.reached_at >= threshold {
                    return None;
                }
                if self
                    .sent_nudges
                    .contains(&(oc.user_id.clone(), oc.checkpoint, sequence))
                {
                    return None;
                }
                let has_later = max_cp
                    .get(oc.user_id.as_str())
                    .is_some_and(|m| *m > oc.checkpoint);
                if has_later {
                    return None;
                }
                Some(PendingNudge {
                    user_id: oc.user_id.clone(),
                    checkpoint_id: oc.checkpoint,
                    email: email.clone(),
                })
            })
            .collect()
    }
}

#[derive(Default)]
pub struct OnboardingState {
    inner: Mutex<OnboardingStore>,
}

impl OnboardingState {
    fn record(&self, payload: CheckpointPayload, now: DateTime<Utc>) {
        self.inner.lock().record_checkpoint(payload, now);
    }

    fn pending_nudges(&self, sequence: i64, now: DateTime<Utc>) -> Vec<PendingNudge> {
        self.inner.lock().pending_nudges(sequence, now)
    }

    #[cfg(test)]
    fn mark_nudge_sent(&self, user_id: &str, checkpoint: i64, sequence: i64) {
        self.inner
            .lock()
            .sent_nudges
            .insert((user_id.to_string(), checkpoint, sequence));
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/onboarding/checkpoint", post(post_checkpoint))
        .route("/onboarding/pending-nudges", get(get_pending_nudges))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointRequestBody {
    #[serde(rename = "checkpointId")]
    checkpoint_id: i64,
    #[serde(rename = "userIdentifier")]
    user_identifier: String,
    #[serde(rename = "identifierType")]
    identifier_type: String,
    action: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    wallet: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

fn validate_checkpoint(body: &CheckpointRequestBody) -> Result<(), String> {
    if body.checkpoint_id < MIN_CHECKPOINT || body.checkpoint_id > MAX_CHECKPOINT {
        return Err("checkpointId must be an integer between 1 and 7".to_string());
    }
    let ulen = body.user_identifier.chars().count();
    if !(1..=MAX_USER_IDENTIFIER_LEN).contains(&ulen) {
        return Err("userIdentifier must be between 1 and 255 characters".to_string());
    }
    if body.identifier_type != "email" && body.identifier_type != "wallet" {
        return Err("identifierType must be one of 'email', 'wallet'".to_string());
    }
    if body.action != "reached" && body.action != "completed" {
        return Err("action must be one of 'reached', 'completed'".to_string());
    }
    if let Some(email) = &body.email {
        if email.chars().count() > MAX_EMAIL_LEN {
            return Err("email must be at most 255 characters".to_string());
        }
    }
    if let Some(wallet) = &body.wallet {
        if wallet.chars().count() > MAX_WALLET_LEN {
            return Err("wallet must be at most 255 characters".to_string());
        }
    }
    if let Some(source) = &body.source {
        if source.chars().count() > MAX_SOURCE_LEN {
            return Err("source must be at most 50 characters".to_string());
        }
    }
    if let Some(metadata) = &body.metadata {
        if !metadata.is_object() {
            return Err("metadata must be an object".to_string());
        }
    }
    Ok(())
}

async fn post_checkpoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(resp) = require_onboarding_bearer(&state, &headers) {
        return resp;
    }

    let req: CheckpointRequestBody = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => return bad_request(&e.to_string()),
    };
    if let Err(msg) = validate_checkpoint(&req) {
        return bad_request(&msg);
    }

    state.onboarding.record(
        CheckpointPayload {
            user_identifier: req.user_identifier,
            identifier_type: req.identifier_type,
            checkpoint_id: req.checkpoint_id,
            action: req.action,
            email: req.email,
            wallet: req.wallet,
            source: req.source,
            metadata: req.metadata,
        },
        Utc::now(),
    );

    (StatusCode::OK, Json(CheckpointAck { success: true })).into_response()
}

#[derive(Debug, Deserialize)]
struct PendingNudgesQuery {
    #[serde(default)]
    sequence: Option<i64>,
}

async fn get_pending_nudges(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PendingNudgesQuery>,
) -> Response {
    if let Err(resp) = require_onboarding_bearer(&state, &headers) {
        return resp;
    }

    let sequence = query.sequence.unwrap_or(1);
    if !(1..=3).contains(&sequence) {
        return bad_request("sequence must be 1, 2, or 3");
    }

    let nudges = state.onboarding.pending_nudges(sequence, Utc::now());
    (
        StatusCode::OK,
        Json(PendingNudgesResponse {
            sequence,
            count: nudges.len() as u64,
            nudges,
        }),
    )
        .into_response()
}

fn require_onboarding_bearer(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let expected = match state.cfg.onboarding_api_key.as_deref() {
        Some(key) if !key.is_empty() => key,
        _ => return Err(unauthorized()),
    };
    let provided = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("");
    if timing_safe_eq(provided, expected) {
        Ok(())
    } else {
        Err(unauthorized())
    }
}

fn timing_safe_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn unauthorized() -> Response {
    json_response(
        StatusCode::UNAUTHORIZED,
        ErrorMessage {
            error: "Unauthorized".to_string(),
        },
    )
}

fn bad_request(msg: &str) -> Response {
    json_response(
        StatusCode::BAD_REQUEST,
        ErrorMessage {
            error: msg.to_string(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_700_000_000 + secs, 0).unwrap()
    }

    fn reached(
        user: &str,
        id_type: &str,
        checkpoint: i64,
        email: Option<&str>,
        wallet: Option<&str>,
    ) -> CheckpointPayload {
        CheckpointPayload {
            user_identifier: user.to_string(),
            identifier_type: id_type.to_string(),
            checkpoint_id: checkpoint,
            action: "reached".to_string(),
            email: email.map(str::to_string),
            wallet: wallet.map(str::to_string),
            source: None,
            metadata: None,
        }
    }

    #[test]
    fn reached_then_pending_after_interval() {
        let mut store = OnboardingStore::default();
        store.record_checkpoint(reached("a@b.com", "email", 1, Some("a@b.com"), None), t(0));

        assert!(store.pending_nudges(1, t(60)).is_empty());
        assert_eq!(
            store.pending_nudges(1, t(13 * 3600)),
            vec![PendingNudge {
                user_id: "a@b.com".to_string(),
                checkpoint_id: 1,
                email: "a@b.com".to_string(),
            }]
        );

        assert!(store.pending_nudges(2, t(13 * 3600)).is_empty());
        assert_eq!(store.pending_nudges(2, t(25 * 3600)).len(), 1);
    }

    #[test]
    fn checkpoint_without_email_is_not_pending() {
        let mut store = OnboardingStore::default();
        store.record_checkpoint(reached("0xabc", "wallet", 1, None, None), t(0));
        assert!(store.pending_nudges(1, t(13 * 3600)).is_empty());
    }

    #[test]
    fn reaching_next_checkpoint_completes_previous_and_supersedes_it() {
        let mut store = OnboardingStore::default();
        store.record_checkpoint(reached("a@b.com", "email", 1, Some("a@b.com"), None), t(0));
        store.record_checkpoint(reached("a@b.com", "email", 2, Some("a@b.com"), None), t(10));

        let pending = store.pending_nudges(1, t(13 * 3600));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].checkpoint_id, 2);
    }

    #[test]
    fn completed_action_marks_completion() {
        let mut store = OnboardingStore::default();
        store.record_checkpoint(reached("a@b.com", "email", 3, Some("a@b.com"), None), t(0));
        assert_eq!(store.pending_nudges(1, t(13 * 3600)).len(), 1);

        store.record_checkpoint(
            CheckpointPayload {
                action: "completed".to_string(),
                ..reached("a@b.com", "email", 3, None, None)
            },
            t(20),
        );
        assert!(store.pending_nudges(1, t(13 * 3600)).is_empty());
    }

    #[test]
    fn wallet_checkpoint_resolves_to_email_user_id() {
        let mut store = OnboardingStore::default();
        store.record_checkpoint(
            reached("a@b.com", "email", 1, Some("a@b.com"), Some("0xABC")),
            t(0),
        );
        store.record_checkpoint(reached("0xAbC", "wallet", 2, None, None), t(10));

        let uid2 = store
            .checkpoints
            .iter()
            .find(|r| r.checkpoint == 2)
            .map(|r| r.user_id.clone());
        assert_eq!(uid2.as_deref(), Some("a@b.com"));

        let pending = store.pending_nudges(1, t(13 * 3600));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].checkpoint_id, 2);
        assert_eq!(pending[0].user_id, "a@b.com");
    }

    #[test]
    fn upsert_coalesces_email_and_wallet() {
        let mut store = OnboardingStore::default();
        store.record_checkpoint(reached("u", "wallet", 1, None, Some("0xabc")), t(0));
        store.record_checkpoint(reached("u", "wallet", 1, Some("late@b.com"), None), t(5));

        let row = store
            .checkpoints
            .iter()
            .find(|r| r.checkpoint == 1)
            .unwrap();
        assert_eq!(row.email.as_deref(), Some("late@b.com"));
        assert_eq!(row.wallet.as_deref(), Some("0xabc"));
    }

    #[test]
    fn sent_nudge_is_excluded() {
        let state = OnboardingState::default();
        state.record(reached("a@b.com", "email", 1, Some("a@b.com"), None), t(0));
        assert_eq!(state.pending_nudges(1, t(13 * 3600)).len(), 1);
        state.mark_nudge_sent("a@b.com", 1, 1);
        assert!(state.pending_nudges(1, t(13 * 3600)).is_empty());
        assert_eq!(state.pending_nudges(2, t(25 * 3600)).len(), 1);
    }

    #[test]
    fn pending_nudges_is_linear_not_quadratic() {
        let mut store = OnboardingStore::default();
        for i in 0..20_000 {
            store.checkpoints.push(CheckpointRow {
                user_id: format!("user-{i:05}"),
                id_type: "email".into(),
                email: Some(format!("u{i}@example.com")),
                wallet: None,
                checkpoint: 1,
                reached_at: t(0),
                completed_at: None,
                source: None,
                metadata: None,
            });
        }
        let start = std::time::Instant::now();
        let nudges = store.pending_nudges(1, t(13 * 3600)); // 13h > 12h threshold; every row survives to has_later
        let elapsed = start.elapsed();
        assert_eq!(nudges.len(), 20_000);
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "pending_nudges took {elapsed:?}"
        );
    }

    #[test]
    fn validation_rejects_out_of_range_and_bad_enums() {
        let base = |cp: i64, id_type: &str, action: &str| CheckpointRequestBody {
            checkpoint_id: cp,
            user_identifier: "u".to_string(),
            identifier_type: id_type.to_string(),
            action: action.to_string(),
            email: None,
            wallet: None,
            source: None,
            metadata: None,
        };
        assert!(validate_checkpoint(&base(1, "email", "reached")).is_ok());
        assert!(validate_checkpoint(&base(7, "wallet", "completed")).is_ok());
        assert!(validate_checkpoint(&base(0, "email", "reached")).is_err());
        assert!(validate_checkpoint(&base(8, "email", "reached")).is_err());
        assert!(validate_checkpoint(&base(1, "phone", "reached")).is_err());
        assert!(validate_checkpoint(&base(1, "email", "skipped")).is_err());

        let mut empty_user = base(1, "email", "reached");
        empty_user.user_identifier = String::new();
        assert!(validate_checkpoint(&empty_user).is_err());
    }
}
