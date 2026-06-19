//! MLS messaging delivery-service HTTP endpoints.
//!
//! ADR: `docs/federation/messaging.md`. This catalyst is the MLS delivery
//! service (not a group member): it distributes KeyPackages, routes opaque
//! Welcome/Commit/application ciphertext, persists encrypted history, and
//! serialises epoch advances. It can never decrypt — every byte payload is
//! opaque MLS material.
//!
//! Authorization model:
//!   * key-package publish: signed-fetch authed; publisher must own the
//!     credential it publishes.
//!   * key-package fetch: any authed wallet.
//!   * group history / commit / blob fetch: current group members only.
//!   * message send / commit: current group members only; epoch-advancing
//!     commits additionally must go to the group's `epoch_author`.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use base64::Engine;
use serde::Deserialize;
use serde_json::json;

use crate::auth_chain::require_signer;
use crate::http::{forbidden, unauthorized, ApiError};
use crate::mls;
use crate::AppState;

#[inline]
fn bad_request(msg: impl Into<String>) -> ApiError {
    ApiError::bad_request(msg)
}

/// Decode a base64 (standard, padded) field, bounded to 256 KiB so a caller
/// can't OOM us.
fn b64_field(v: &serde_json::Value, key: &str) -> Result<Vec<u8>, ApiError> {
    let s = v
        .get(key)
        .and_then(|x| x.as_str())
        .ok_or_else(|| bad_request(format!("missing base64 field `{key}`")))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| bad_request(format!("field `{key}` is not valid base64")))?;
    if bytes.is_empty() || bytes.len() > 256 * 1024 {
        return Err(bad_request(format!("field `{key}` length out of range")));
    }
    Ok(bytes)
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn is_eth_address(addr: &str) -> bool {
    addr.len() == 42
        && addr.starts_with("0x")
        && addr[2..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Require a signed-fetch identity for `method path`; returns the lowercase signer.
fn auth(headers: &HeaderMap, method: &str, path: &str) -> Result<String, ApiError> {
    require_signer(headers, method, path)
        .map(|s| s.to_lowercase())
        .map_err(|e| unauthorized(format!("invalid identity: {e}")))
}

/// True if `wallet` is a current (not-removed) member of `group_id`.
async fn is_member(state: &AppState, group_id: &str, wallet: &str) -> Result<bool, ApiError> {
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mls_group_members \
         WHERE group_id = $1 AND member = $2 AND removed_epoch IS NULL",
    )
    .bind(group_id)
    .bind(wallet)
    .fetch_one(&state.pool)
    .await?;
    Ok(n > 0)
}

/// `POST /mls/key-packages` — publish one-time KeyPackages for the authed
/// identity. Body: `{ "key_packages": ["<base64 MLSMessage>", ..] }`. The
/// publisher must own the credential identity inside each KP.
pub async fn publish_key_packages(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let signer = auth(&headers, "post", "/mls/key-packages")?;
    let payload: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| bad_request(e.to_string()))?;
    let arr = payload
        .get("key_packages")
        .and_then(|v| v.as_array())
        .ok_or_else(|| bad_request("missing `key_packages` array"))?;
    if arr.is_empty() || arr.len() > 100 {
        return Err(bad_request("`key_packages` must contain 1..=100 entries"));
    }

    let mut stored = Vec::new();
    for entry in arr {
        let s = entry
            .as_str()
            .ok_or_else(|| bad_request("`key_packages` entries must be base64 strings"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|_| bad_request("key package is not valid base64"))?;
        if bytes.is_empty() || bytes.len() > 256 * 1024 {
            return Err(bad_request("key package length out of range"));
        }
        let parsed = mls::parse_key_package(&bytes).map_err(|e| bad_request(e.to_string()))?;

        // Bind the published KP to the authed wallet (accept the address with or
        // without the 0x prefix).
        let cred = String::from_utf8_lossy(&parsed.credential_identity)
            .trim()
            .to_lowercase();
        if !cred.is_empty() && cred != signer && cred != signer.trim_start_matches("0x") {
            return Err(forbidden(
                "key package credential identity does not match the authenticated wallet",
            ));
        }

        sqlx::query(
            "INSERT INTO mls_key_packages (owner, ref_hash, ciphersuite, key_package) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (ref_hash) DO NOTHING",
        )
        .bind(&signer)
        .bind(&parsed.ref_hash)
        .bind(parsed.ciphersuite_id as i32)
        .bind(&bytes)
        .execute(&state.pool)
        .await?;
        stored.push(parsed.ref_hash);
    }

    Ok(Json(json!({ "published": stored.len(), "refs": stored })))
}

/// `GET /mls/key-packages/{owner}` — claim one unconsumed KeyPackage for
/// `owner` (single-use, marked consumed atomically). Authed (any wallet).
pub async fn claim_key_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(owner): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner = owner.to_lowercase();
    if !is_eth_address(&owner) {
        return Err(bad_request("owner must be an eth address"));
    }
    let _claimer = auth(&headers, "get", &format!("/mls/key-packages/{owner}"))?;

    let row = sqlx::query_as::<_, (String, Vec<u8>, i32)>(
        "UPDATE mls_key_packages SET consumed_at = now() \
         WHERE id = ( \
            SELECT id FROM mls_key_packages \
            WHERE owner = $1 AND consumed_at IS NULL \
            ORDER BY created_at ASC LIMIT 1 FOR UPDATE SKIP LOCKED \
         ) RETURNING ref_hash, key_package, ciphersuite",
    )
    .bind(&owner)
    .fetch_optional(&state.pool)
    .await?;

    match row {
        Some((ref_hash, kp, cs)) => Ok(Json(json!({
            "owner": owner,
            "ref": ref_hash,
            "ciphersuite": cs,
            "key_package": b64(&kp),
        }))),
        None => Err(ApiError::schema(
            404,
            json!({ "error": "no key package available", "owner": owner, "last_resort": false }),
        )),
    }
}

/// `GET /mls/key-packages/{owner}/count` — unconsumed KPs remaining. Authed (any wallet).
pub async fn key_package_count(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(owner): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner = owner.to_lowercase();
    if !is_eth_address(&owner) {
        return Err(bad_request("owner must be an eth address"));
    }
    let _ = auth(&headers, "get", &format!("/mls/key-packages/{owner}/count"))?;
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mls_key_packages WHERE owner = $1 AND consumed_at IS NULL",
    )
    .bind(&owner)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "owner": owner, "available": n })))
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupBody {
    /// hex of the MLS GroupId (32-byte opaque) the creator generated locally.
    pub group_id: String,
    /// 'dm' | 'channel'
    pub group_kind: String,
    /// for channels: the owning community id. Optional for DMs.
    #[serde(default)]
    pub community_id: Option<String>,
    /// initial member wallets (creator included). For a DM this is exactly two.
    pub initial_members: Vec<String>,
    /// base64 MLSMessage(Commit). Optional at create; may be supplied via /commits.
    #[serde(default)]
    pub initial_commit: Option<String>,
    /// base64 MLSMessage(Welcome) for the added initial members. Optional.
    #[serde(default)]
    pub welcome: Option<String>,
}

/// `POST /mls/groups` — register a new MLS group; the creator becomes the
/// epoch-author. Authed; creator must be in `initial_members`.
pub async fn create_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let creator = auth(&headers, "post", "/mls/groups")?;
    let b: CreateGroupBody =
        serde_json::from_slice(&body).map_err(|e| bad_request(e.to_string()))?;

    let group_id = b.group_id.to_lowercase();
    if group_id.is_empty() || hex::decode(&group_id).is_err() {
        return Err(bad_request("group_id must be hex"));
    }
    if b.group_kind != "dm" && b.group_kind != "channel" {
        return Err(bad_request("group_kind must be 'dm' or 'channel'"));
    }
    let members: Vec<String> = b
        .initial_members
        .iter()
        .map(|m| m.to_lowercase())
        .collect();
    if members.iter().any(|m| !is_eth_address(m)) {
        return Err(bad_request("initial_members must be eth addresses"));
    }
    if !members.contains(&creator) {
        return Err(forbidden("creator must be among initial_members"));
    }
    if b.group_kind == "dm" && members.len() != 2 {
        return Err(bad_request("a 'dm' group must have exactly two members"));
    }

    // epoch-author = creator's home catalyst. Single-node default: our peer id.
    let epoch_author = std::env::var("FED_PEER_ID").unwrap_or_else(|_| "local".to_string());

    let mut tx = state.pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO mls_groups \
            (group_id, creator, group_kind, community_id, epoch_author, current_epoch, ciphersuite) \
         VALUES ($1, $2, $3, $4, $5, 0, $6) ON CONFLICT (group_id) DO NOTHING",
    )
    .bind(&group_id)
    .bind(&creator)
    .bind(&b.group_kind)
    .bind(&b.community_id)
    .bind(&epoch_author)
    .bind(mls::PINNED_CIPHERSUITE_ID as i32)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(ApiError::http(409, "group already exists"));
    }

    for m in &members {
        sqlx::query(
            "INSERT INTO mls_group_members (group_id, member, added_epoch) \
             VALUES ($1, $2, 0) ON CONFLICT DO NOTHING",
        )
        .bind(&group_id)
        .bind(m)
        .execute(&mut *tx)
        .await?;
    }

    if let Some(c) = &b.initial_commit {
        let commit_bytes = base64::engine::general_purpose::STANDARD
            .decode(c)
            .map_err(|_| bad_request("initial_commit is not valid base64"))?;
        mls::parse_commit_routing(&commit_bytes).map_err(|e| bad_request(e.to_string()))?;
        let welcome_bytes = match &b.welcome {
            Some(w) => Some(
                base64::engine::general_purpose::STANDARD
                    .decode(w)
                    .map_err(|_| bad_request("welcome is not valid base64"))?,
            ),
            None => None,
        };
        let commit_hash = mls::content_hash(&commit_bytes);
        sqlx::query(
            "INSERT INTO mls_commits \
                (group_id, epoch, commit_bytes, welcome_bytes, committer, commit_hash, signed_at) \
             VALUES ($1, 0, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING",
        )
        .bind(&group_id)
        .bind(&commit_bytes)
        .bind(welcome_bytes.as_deref())
        .bind(&creator)
        .bind(&commit_hash)
        .bind(chrono::Utc::now().timestamp())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Json(json!({
        "group_id": group_id,
        "group_kind": b.group_kind,
        "epoch_author": epoch_author,
        "current_epoch": 0,
        "ciphersuite": mls::PINNED_CIPHERSUITE_ID,
        "members": members,
    })))
}

#[derive(Debug, Deserialize)]
pub struct CommitBody {
    /// target epoch this commit advances the group to.
    pub epoch: i64,
    /// base64 MLSMessage(Commit/handshake).
    pub commit: String,
    /// base64 MLSMessage(Welcome) for members added in this commit. Optional.
    #[serde(default)]
    pub welcome: Option<String>,
    /// membership delta for the authz roster (server can't read the commit):
    /// wallets added / removed by this commit. Optional; defaults to none.
    #[serde(default)]
    pub added_members: Vec<String>,
    #[serde(default)]
    pub removed_members: Vec<String>,
}

/// `POST /mls/groups/{group_id}/commits` — submit an epoch-advancing commit.
/// Must be sent to the group's epoch-author. Authed; sender must be a current
/// member; epoch must be exactly `current_epoch + 1`.
pub async fn submit_commit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let group_id = group_id.to_lowercase();
    let signer = auth(&headers, "post", &format!("/mls/groups/{group_id}/commits"))?;
    let b: CommitBody = serde_json::from_slice(&body).map_err(|e| bad_request(e.to_string()))?;

    let g = sqlx::query_as::<_, (String, i64)>(
        "SELECT epoch_author, current_epoch FROM mls_groups WHERE group_id = $1",
    )
    .bind(&group_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("group not found"))?;
    let (epoch_author, current_epoch) = g;

    if !is_member(&state, &group_id, &signer).await? {
        return Err(forbidden("only group members may submit commits"));
    }

    let our_peer = std::env::var("FED_PEER_ID").unwrap_or_else(|_| "local".to_string());
    if epoch_author != our_peer {
        return Err(ApiError::schema(
            409,
            json!({
                "error": "wrong epoch author",
                "message": "submit epoch-advancing commits to the group's epoch-author catalyst",
                "epoch_author": epoch_author,
            }),
        ));
    }

    if b.epoch != current_epoch + 1 {
        return Err(ApiError::schema(
            409,
            json!({
                "error": "epoch conflict",
                "message": "commit epoch must be current_epoch + 1",
                "current_epoch": current_epoch,
                "submitted_epoch": b.epoch,
            }),
        ));
    }

    let commit_bytes = base64::engine::general_purpose::STANDARD
        .decode(&b.commit)
        .map_err(|_| bad_request("commit is not valid base64"))?;
    if commit_bytes.len() > 256 * 1024 {
        return Err(bad_request("commit too large"));
    }
    mls::parse_commit_routing(&commit_bytes).map_err(|e| bad_request(e.to_string()))?;
    let welcome_bytes = match &b.welcome {
        Some(w) => Some(
            base64::engine::general_purpose::STANDARD
                .decode(w)
                .map_err(|_| bad_request("welcome is not valid base64"))?,
        ),
        None => None,
    };
    let commit_hash = mls::content_hash(&commit_bytes);
    let now = chrono::Utc::now().timestamp();

    let mut tx = state.pool.begin().await?;
    // Serialise the epoch advance: re-check current_epoch under the row lock.
    let locked: i64 =
        sqlx::query_scalar("SELECT current_epoch FROM mls_groups WHERE group_id = $1 FOR UPDATE")
            .bind(&group_id)
            .fetch_one(&mut *tx)
            .await?;
    if b.epoch != locked + 1 {
        return Err(ApiError::http(409, "epoch advanced concurrently; retry"));
    }

    sqlx::query(
        "INSERT INTO mls_commits \
            (group_id, epoch, commit_bytes, welcome_bytes, committer, commit_hash, signed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&group_id)
    .bind(b.epoch)
    .bind(&commit_bytes)
    .bind(welcome_bytes.as_deref())
    .bind(&signer)
    .bind(&commit_hash)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE mls_groups SET current_epoch = $2, last_commit_hash = $3, updated_at = now() \
         WHERE group_id = $1",
    )
    .bind(&group_id)
    .bind(b.epoch)
    .bind(&commit_hash)
    .execute(&mut *tx)
    .await?;

    for m in &b.added_members {
        let m = m.to_lowercase();
        if !is_eth_address(&m) {
            continue;
        }
        sqlx::query(
            "INSERT INTO mls_group_members (group_id, member, added_epoch) VALUES ($1, $2, $3) \
             ON CONFLICT (group_id, member) DO UPDATE SET removed_epoch = NULL, added_epoch = $3",
        )
        .bind(&group_id)
        .bind(&m)
        .bind(b.epoch)
        .execute(&mut *tx)
        .await?;
    }
    for m in &b.removed_members {
        let m = m.to_lowercase();
        sqlx::query(
            "UPDATE mls_group_members SET removed_epoch = $3 \
             WHERE group_id = $1 AND member = $2 AND removed_epoch IS NULL",
        )
        .bind(&group_id)
        .bind(&m)
        .bind(b.epoch)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Json(json!({
        "group_id": group_id,
        "epoch": b.epoch,
        "commit_hash": commit_hash,
    })))
}

#[derive(Debug, Deserialize)]
pub struct CommitsQuery {
    /// fetch commits with epoch >= from (catch-up). Default 0.
    #[serde(default)]
    pub from: i64,
}

/// `GET /mls/groups/{group_id}/commits?from=N` — fetch handshake commits from
/// epoch N onward (catch-up). Members only.
pub async fn fetch_commits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Query(q): Query<CommitsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let group_id = group_id.to_lowercase();
    let signer = auth(&headers, "get", &format!("/mls/groups/{group_id}/commits"))?;
    if !is_member(&state, &group_id, &signer).await? {
        return Err(forbidden("only group members may fetch commits"));
    }

    let rows = sqlx::query_as::<_, (i64, Vec<u8>, Option<Vec<u8>>, String, i64)>(
        "SELECT epoch, commit_bytes, welcome_bytes, commit_hash, signed_at \
         FROM mls_commits WHERE group_id = $1 AND epoch >= $2 ORDER BY epoch ASC LIMIT 500",
    )
    .bind(&group_id)
    .bind(q.from)
    .fetch_all(&state.pool)
    .await?;

    let commits: Vec<_> = rows
        .into_iter()
        .map(|(epoch, commit, welcome, hash, signed_at)| {
            json!({
                "epoch": epoch,
                "commit": b64(&commit),
                "welcome": welcome.as_deref().map(b64),
                "commit_hash": hash,
                "signed_at": signed_at,
            })
        })
        .collect();

    Ok(Json(json!({ "group_id": group_id, "commits": commits })))
}

/// `POST /mls/groups/{group_id}/messages` — submit an application-message
/// ciphertext. Body: `{ "ciphertext": "<base64 MLSMessage(PrivateMessage)>" }`.
/// The server parses the framing only (to confirm group_id + epoch),
/// content-addresses + dedups the ciphertext, and records an ordered ref.
/// Members only.
pub async fn send_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let group_id = group_id.to_lowercase();
    let signer = auth(&headers, "post", &format!("/mls/groups/{group_id}/messages"))?;

    let g = sqlx::query_as::<_, (i64,)>("SELECT current_epoch FROM mls_groups WHERE group_id = $1")
        .bind(&group_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("group not found"))?;
    let current_epoch = g.0;

    if !is_member(&state, &group_id, &signer).await? {
        return Err(forbidden("only group members may send messages"));
    }

    let payload: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| bad_request(e.to_string()))?;
    let ciphertext = b64_field(&payload, "ciphertext")?;

    // Framing check: confirm this ciphertext is for THIS group. We never decrypt.
    let routing = mls::parse_message_routing(&ciphertext).map_err(|e| bad_request(e.to_string()))?;
    if routing.group_id_hex.to_lowercase() != group_id {
        return Err(bad_request(
            "ciphertext group_id does not match the URL group",
        ));
    }
    // The message epoch may be behind during catch-up, but never ahead.
    if routing.epoch as i64 > current_epoch {
        return Err(ApiError::schema(
            409,
            json!({
                "error": "epoch ahead",
                "message": "message epoch is ahead of the group's current epoch; submit the commit first",
                "current_epoch": current_epoch,
                "message_epoch": routing.epoch,
            }),
        ));
    }

    let ciphertext_hash = mls::content_hash(&ciphertext);
    let now = chrono::Utc::now().timestamp();
    // Content-address the ref deterministically from its routing fields so
    // duplicate submissions dedup.
    let signature_hash = mls::content_hash(
        format!("{group_id}:{}:{signer}:{ciphertext_hash}", routing.epoch).as_bytes(),
    );

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO mls_message_blobs (ciphertext_hash, ciphertext) VALUES ($1, $2) \
         ON CONFLICT (ciphertext_hash) DO NOTHING",
    )
    .bind(&ciphertext_hash)
    .bind(&ciphertext)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO mls_message_refs \
            (signature_hash, group_id, author, epoch, ciphertext_hash, signed_at) \
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&signature_hash)
    .bind(&group_id)
    .bind(&signer)
    .bind(routing.epoch as i64)
    .bind(&ciphertext_hash)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // TODO(federation): when FED_GOSSIP=nats, publish a Signed<MessageRef> here
    // (catalyrst-fed GossipPublisher) so peer catalysts converge. Single-node is
    // correct as-is: LiveKit data-channel handles live delivery, this row is
    // durable history.

    Ok(Json(json!({
        "group_id": group_id,
        "epoch": routing.epoch,
        "ciphertext_hash": ciphertext_hash,
        "signature_hash": signature_hash,
        "signed_at": now,
    })))
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// page back from this received_at (unix seconds), exclusive. Default: now.
    #[serde(default)]
    pub before: Option<i64>,
    /// max rows, 1..=200, default 50.
    #[serde(default)]
    pub limit: Option<i64>,
}

/// `GET /mls/groups/{group_id}/messages` — fetch encrypted history (newest
/// first), paginated; ciphertext inline. Members only.
pub async fn fetch_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let group_id = group_id.to_lowercase();
    let signer = auth(&headers, "get", &format!("/mls/groups/{group_id}/messages"))?;
    if !is_member(&state, &group_id, &signer).await? {
        return Err(forbidden("only group members may fetch history"));
    }

    let before = q.before.unwrap_or(i64::MAX);
    let limit = q.limit.unwrap_or(50).clamp(1, 200);

    let rows = sqlx::query_as::<_, (String, String, i64, i64, Vec<u8>)>(
        "SELECT r.signature_hash, r.author, r.epoch, r.signed_at, b.ciphertext \
         FROM mls_message_refs r JOIN mls_message_blobs b ON b.ciphertext_hash = r.ciphertext_hash \
         WHERE r.group_id = $1 AND extract(epoch FROM r.received_at)::bigint < $2 \
         ORDER BY r.received_at DESC LIMIT $3",
    )
    .bind(&group_id)
    .bind(before)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;

    let messages: Vec<_> = rows
        .into_iter()
        .map(|(sig, author, epoch, signed_at, ct)| {
            json!({
                "signature_hash": sig,
                "author": author,
                "epoch": epoch,
                "signed_at": signed_at,
                "ciphertext": b64(&ct),
            })
        })
        .collect();

    Ok(Json(json!({ "group_id": group_id, "messages": messages })))
}

/// `GET /mls/blobs/{hash}` — fetch a single ciphertext blob by content hash.
/// The caller must be a current member of some group that references it.
pub async fn fetch_blob(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(hash): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let hash = hash.to_lowercase();
    let signer = auth(&headers, "get", &format!("/mls/blobs/{hash}"))?;

    let allowed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mls_message_refs r \
         JOIN mls_group_members m ON m.group_id = r.group_id \
         WHERE r.ciphertext_hash = $1 AND m.member = $2 AND m.removed_epoch IS NULL",
    )
    .bind(&hash)
    .bind(&signer)
    .fetch_one(&state.pool)
    .await?;
    if allowed == 0 {
        return Err(forbidden("not authorized for this blob"));
    }

    let blob: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT ciphertext FROM mls_message_blobs WHERE ciphertext_hash = $1")
            .bind(&hash)
            .fetch_optional(&state.pool)
            .await?;
    match blob {
        Some(b) => Ok(Json(json!({ "hash": hash, "ciphertext": b64(&b) }))),
        None => Err(ApiError::not_found("blob not found")),
    }
}
