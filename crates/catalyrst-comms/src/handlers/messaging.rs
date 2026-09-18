use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use base64::Engine;
use catalyrst_types::is_eth_address;
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

async fn auth(headers: &HeaderMap, method: &str, path: &str) -> Result<String, ApiError> {
    require_signer(headers, method, path)
        .await
        .map(|s| s.as_str().to_string())
        .map_err(|e| unauthorized(format!("invalid identity: {e}")))
}

fn authorized_community_binding(
    state: &AppState,
    headers: &HeaderMap,
    requested: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let Some(requested) = requested.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let presented_service_token = state
        .gatekeeper_auth_token
        .as_deref()
        .zip(crate::moderator::bearer_token(headers))
        .map(|(expected, token)| crate::moderator::timing_safe_eq(&token, expected))
        .unwrap_or(false);
    if !presented_service_token {
        return Err(forbidden(
            "a wallet signature cannot prove community membership; community-scoped groups may only be created with the platform service token",
        ));
    }
    Ok(Some(requested.to_string()))
}

/// Membership resolved in the same statement as the rows it gates; the LATERAL
/// join yields one row even for a non-member so the two answers stay distinct.
const MEMBER_EXISTS: &str = "EXISTS (SELECT 1 FROM mls_group_members \
     WHERE group_id = $1 AND member = $2 AND removed_epoch IS NULL)";

pub async fn publish_key_packages(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let signer = auth(&headers, "post", "/mls/key-packages").await?;
    let payload: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| bad_request(e.to_string()))?;
    let arr = payload
        .get("key_packages")
        .and_then(|v| v.as_array())
        .ok_or_else(|| bad_request("missing `key_packages` array"))?;
    if arr.is_empty() || arr.len() > 100 {
        return Err(bad_request("`key_packages` must contain 1..=100 entries"));
    }

    let mut stored: Vec<String> = Vec::new();
    let mut ciphersuites: Vec<i32> = Vec::new();
    let mut packages: Vec<Vec<u8>> = Vec::new();
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

        let cred = String::from_utf8_lossy(&parsed.credential_identity)
            .trim()
            .to_lowercase();
        if !cred.is_empty() && cred != signer && cred != signer.trim_start_matches("0x") {
            return Err(forbidden(
                "key package credential identity does not match the authenticated wallet",
            ));
        }

        stored.push(parsed.ref_hash);
        ciphersuites.push(parsed.ciphersuite_id as i32);
        packages.push(bytes);
    }

    sqlx::query(
        "INSERT INTO mls_key_packages (owner, ref_hash, ciphersuite, key_package) \
         SELECT $1, k.ref_hash, k.ciphersuite, k.key_package \
         FROM unnest($2::text[], $3::int4[], $4::bytea[]) AS k(ref_hash, ciphersuite, key_package) \
         ON CONFLICT (ref_hash) DO NOTHING",
    )
    .bind(&signer)
    .bind(&stored)
    .bind(&ciphersuites)
    .bind(&packages)
    .execute(&state.pool)
    .await?;

    Ok(Json(json!({ "published": stored.len(), "refs": stored })))
}

pub async fn claim_key_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(owner): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner = owner.to_lowercase();
    if !is_eth_address(&owner) {
        return Err(bad_request("owner must be an eth address"));
    }
    let _claimer = auth(&headers, "get", &format!("/mls/key-packages/{owner}")).await?;

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
        None => Err(ApiError::labeled(
            404,
            "no key package available",
            format!("no key package available for {owner}"),
        )),
    }
}

pub async fn key_package_count(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(owner): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner = owner.to_lowercase();
    if !is_eth_address(&owner) {
        return Err(bad_request("owner must be an eth address"));
    }
    let _ = auth(&headers, "get", &format!("/mls/key-packages/{owner}/count")).await?;
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
    pub group_id: String,

    pub group_kind: String,

    #[serde(default)]
    pub community_id: Option<String>,

    pub initial_members: Vec<String>,

    #[serde(default)]
    pub initial_commit: Option<String>,

    #[serde(default)]
    pub welcome: Option<String>,
}

pub async fn create_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let creator = auth(&headers, "post", "/mls/groups").await?;
    let b: CreateGroupBody =
        serde_json::from_slice(&body).map_err(|e| bad_request(e.to_string()))?;

    let group_id = b.group_id.to_lowercase();
    if group_id.is_empty() || hex::decode(&group_id).is_err() {
        return Err(bad_request("group_id must be hex"));
    }
    if b.group_kind != "dm" && b.group_kind != "channel" {
        return Err(bad_request("group_kind must be 'dm' or 'channel'"));
    }
    let members: Vec<String> = b.initial_members.iter().map(|m| m.to_lowercase()).collect();
    if members.iter().any(|m| !is_eth_address(m)) {
        return Err(bad_request("initial_members must be eth addresses"));
    }
    if !members.contains(&creator) {
        return Err(forbidden("creator must be among initial_members"));
    }
    if b.group_kind == "dm" && members.len() != 2 {
        return Err(bad_request("a 'dm' group must have exactly two members"));
    }

    let community_id = authorized_community_binding(&state, &headers, b.community_id.as_deref())?;

    let epoch_author = state.fed_peer_id.clone();

    let (commit_bytes, welcome_bytes, commit_hash) = match &b.initial_commit {
        Some(c) => {
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
            (Some(commit_bytes), welcome_bytes, Some(commit_hash))
        }
        None => (None, None, None),
    };

    let created: bool = sqlx::query_scalar(
        "WITH g AS ( \
            INSERT INTO mls_groups \
                (group_id, creator, group_kind, community_id, epoch_author, current_epoch, ciphersuite) \
            VALUES ($1, $2, $3, $4, $5, 0, $6) ON CONFLICT (group_id) DO NOTHING \
            RETURNING group_id \
         ), m AS ( \
            INSERT INTO mls_group_members (group_id, member, added_epoch) \
            SELECT g.group_id, x.member, 0 FROM g, unnest($7::text[]) AS x(member) \
            ON CONFLICT DO NOTHING \
         ), c AS ( \
            INSERT INTO mls_commits \
                (group_id, epoch, commit_bytes, welcome_bytes, committer, commit_hash, signed_at) \
            SELECT g.group_id, 0, $8, $9, $2, $10, $11 FROM g WHERE $8::bytea IS NOT NULL \
            ON CONFLICT DO NOTHING \
         ) \
         SELECT EXISTS (SELECT 1 FROM g)",
    )
    .bind(&group_id)
    .bind(&creator)
    .bind(&b.group_kind)
    .bind(&community_id)
    .bind(&epoch_author)
    .bind(mls::PINNED_CIPHERSUITE_ID as i32)
    .bind(&members)
    .bind(commit_bytes.as_deref())
    .bind(welcome_bytes.as_deref())
    .bind(&commit_hash)
    .bind(chrono::Utc::now().timestamp())
    .fetch_one(&state.pool)
    .await?;
    if !created {
        return Err(ApiError::http(409, "group already exists"));
    }

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
    pub epoch: i64,

    pub commit: String,

    #[serde(default)]
    pub welcome: Option<String>,

    #[serde(default)]
    pub added_members: Vec<String>,
    #[serde(default)]
    pub removed_members: Vec<String>,
}

pub async fn submit_commit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let group_id = group_id.to_lowercase();
    let signer = auth(&headers, "post", &format!("/mls/groups/{group_id}/commits")).await?;
    let b: CommitBody = serde_json::from_slice(&body).map_err(|e| bad_request(e.to_string()))?;

    let g = sqlx::query_as::<_, (String, i64, bool)>(
        "SELECT g.epoch_author, g.current_epoch, \
            EXISTS(SELECT 1 FROM mls_group_members m \
                   WHERE m.group_id = g.group_id AND m.member = $2 \
                     AND m.removed_epoch IS NULL) \
         FROM mls_groups g WHERE g.group_id = $1",
    )
    .bind(&group_id)
    .bind(&signer)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("group not found"))?;
    let (epoch_author, current_epoch, is_member) = g;

    if !is_member {
        return Err(forbidden("only group members may submit commits"));
    }

    let our_peer = state.fed_peer_id.as_str();
    if epoch_author != our_peer {
        return Err(ApiError::labeled(
            409,
            "wrong epoch author",
            format!("submit epoch-advancing commits to the group's epoch-author catalyst ({epoch_author})"),
        ));
    }

    if b.epoch != current_epoch + 1 {
        return Err(ApiError::labeled(
            409,
            "epoch conflict",
            format!(
                "commit epoch must be current_epoch + 1 (current_epoch={current_epoch}, submitted_epoch={})",
                b.epoch
            ),
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

    let mut added: Vec<String> = b
        .added_members
        .iter()
        .map(|m| m.to_lowercase())
        .filter(|m| is_eth_address(m))
        .collect();
    added.sort();
    added.dedup();
    let removed: Vec<String> = b.removed_members.iter().map(|m| m.to_lowercase()).collect();

    // FOR UPDATE re-reads the row after a concurrent commit, so the guard sees it.
    let locked: i64 = sqlx::query_scalar(
        "WITH g AS ( \
            SELECT current_epoch FROM mls_groups WHERE group_id = $1 FOR UPDATE \
         ), ok AS ( \
            SELECT current_epoch FROM g WHERE current_epoch + 1 = $2 \
         ), c AS ( \
            INSERT INTO mls_commits \
                (group_id, epoch, commit_bytes, welcome_bytes, committer, commit_hash, signed_at) \
            SELECT $1, $2, $3, $4, $5, $6, $7 FROM ok \
         ), u AS ( \
            UPDATE mls_groups SET current_epoch = $2, last_commit_hash = $6, updated_at = now() \
            FROM ok WHERE mls_groups.group_id = $1 \
         ), a AS ( \
            INSERT INTO mls_group_members (group_id, member, added_epoch) \
            SELECT $1, x.member, $2 FROM ok, unnest($8::text[]) AS x(member) \
            ON CONFLICT (group_id, member) DO UPDATE \
                SET removed_epoch = NULL, added_epoch = EXCLUDED.added_epoch \
         ), r AS ( \
            UPDATE mls_group_members SET removed_epoch = $2 \
            FROM ok WHERE group_id = $1 AND member = ANY($9::text[]) AND removed_epoch IS NULL \
         ) \
         SELECT current_epoch FROM g",
    )
    .bind(&group_id)
    .bind(b.epoch)
    .bind(&commit_bytes)
    .bind(welcome_bytes.as_deref())
    .bind(&signer)
    .bind(&commit_hash)
    .bind(now)
    .bind(&added)
    .bind(&removed)
    .fetch_one(&state.pool)
    .await?;
    if b.epoch != locked + 1 {
        return Err(ApiError::http(409, "epoch advanced concurrently; retry"));
    }

    Ok(Json(json!({
        "group_id": group_id,
        "epoch": b.epoch,
        "commit_hash": commit_hash,
    })))
}

#[derive(Debug, Deserialize)]
pub struct CommitsQuery {
    #[serde(default)]
    pub from: i64,
}

pub async fn fetch_commits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Query(q): Query<CommitsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let group_id = group_id.to_lowercase();
    let signer = auth(&headers, "get", &format!("/mls/groups/{group_id}/commits")).await?;

    let rows = sqlx::query_as::<
        _,
        (
            bool,
            Option<i64>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
            Option<String>,
            Option<i64>,
        ),
    >(sqlx::AssertSqlSafe(format!(
        "SELECT m.ok, c.epoch, c.commit_bytes, c.welcome_bytes, c.commit_hash, c.signed_at \
             FROM (SELECT {MEMBER_EXISTS} AS ok) m \
             LEFT JOIN LATERAL ( \
                SELECT epoch, commit_bytes, welcome_bytes, commit_hash, signed_at \
                FROM mls_commits WHERE m.ok AND group_id = $1 AND epoch >= $3 \
                ORDER BY epoch ASC LIMIT 500 \
             ) c ON true ORDER BY c.epoch ASC"
    )))
    .bind(&group_id)
    .bind(&signer)
    .bind(q.from)
    .fetch_all(&state.pool)
    .await?;
    if !rows.first().is_some_and(|row| row.0) {
        return Err(forbidden("only group members may fetch commits"));
    }

    let commits: Vec<_> = rows
        .into_iter()
        .filter_map(|(_, epoch, commit, welcome, hash, signed_at)| {
            Some(json!({
                "epoch": epoch?,
                "commit": b64(&commit?),
                "welcome": welcome.as_deref().map(b64),
                "commit_hash": hash?,
                "signed_at": signed_at?,
            }))
        })
        .collect();

    Ok(Json(json!({ "group_id": group_id, "commits": commits })))
}

pub async fn send_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let group_id = group_id.to_lowercase();
    let signer = auth(
        &headers,
        "post",
        &format!("/mls/groups/{group_id}/messages"),
    )
    .await?;

    let row = sqlx::query_as::<_, (i64, bool)>(
        "SELECT g.current_epoch, \
            EXISTS(SELECT 1 FROM mls_group_members m \
                   WHERE m.group_id = g.group_id AND m.member = $2 \
                     AND m.removed_epoch IS NULL) \
         FROM mls_groups g WHERE g.group_id = $1",
    )
    .bind(&group_id)
    .bind(&signer)
    .fetch_optional(&state.pool)
    .await?;
    let Some((current_epoch, is_member)) = row else {
        return Err(ApiError::not_found("group not found"));
    };
    if !is_member {
        return Err(forbidden("only group members may send messages"));
    }

    let payload: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| bad_request(e.to_string()))?;
    let ciphertext = b64_field(&payload, "ciphertext")?;

    let routing =
        mls::parse_message_routing(&ciphertext).map_err(|e| bad_request(e.to_string()))?;
    if routing.group_id_hex.to_lowercase() != group_id {
        return Err(bad_request(
            "ciphertext group_id does not match the URL group",
        ));
    }

    if routing.epoch as i64 > current_epoch {
        return Err(ApiError::labeled(
            409,
            "epoch ahead",
            format!(
                "message epoch is ahead of the group's current epoch; submit the commit first (current_epoch={current_epoch}, message_epoch={})",
                routing.epoch
            ),
        ));
    }

    let ciphertext_hash = mls::content_hash(&ciphertext);
    let now = chrono::Utc::now().timestamp();

    let signature_hash = mls::content_hash(
        format!("{group_id}:{}:{signer}:{ciphertext_hash}", routing.epoch).as_bytes(),
    );

    sqlx::query(
        "WITH blob AS ( \
            INSERT INTO mls_message_blobs (ciphertext_hash, ciphertext) VALUES ($5, $7) \
            ON CONFLICT (ciphertext_hash) DO NOTHING \
         ) \
         INSERT INTO mls_message_refs \
            (signature_hash, group_id, author, epoch, ciphertext_hash, signed_at) \
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&signature_hash)
    .bind(&group_id)
    .bind(&signer)
    .bind(routing.epoch as i64)
    .bind(&ciphertext_hash)
    .bind(now)
    .bind(&ciphertext)
    .execute(&state.pool)
    .await?;

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
    #[serde(default)]
    pub before: Option<i64>,

    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn fetch_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let group_id = group_id.to_lowercase();
    let signer = auth(&headers, "get", &format!("/mls/groups/{group_id}/messages")).await?;

    let before = q.before.unwrap_or(i64::MAX);
    let limit = q.limit.unwrap_or(50).clamp(1, 200);

    let rows = sqlx::query_as::<_, (bool, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<Vec<u8>>)>(
        sqlx::AssertSqlSafe(format!(
            "SELECT m.ok, r.signature_hash, r.author, r.epoch, r.signed_at, r.ciphertext \
             FROM (SELECT {MEMBER_EXISTS} AS ok) m \
             LEFT JOIN LATERAL ( \
                SELECT r.signature_hash, r.author, r.epoch, r.signed_at, r.received_at, b.ciphertext \
                FROM mls_message_refs r \
                JOIN mls_message_blobs b ON b.ciphertext_hash = r.ciphertext_hash \
                WHERE m.ok AND r.group_id = $1 \
                  AND extract(epoch FROM r.received_at)::bigint < $3 \
                ORDER BY r.received_at DESC LIMIT $4 \
             ) r ON true ORDER BY r.received_at DESC"
        )),
    )
    .bind(&group_id)
    .bind(&signer)
    .bind(before)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    if !rows.first().is_some_and(|row| row.0) {
        return Err(forbidden("only group members may fetch history"));
    }

    let messages: Vec<_> = rows
        .into_iter()
        .filter_map(|(_, sig, author, epoch, signed_at, ct)| {
            Some(json!({
                "signature_hash": sig?,
                "author": author?,
                "epoch": epoch?,
                "signed_at": signed_at?,
                "ciphertext": b64(&ct?),
            }))
        })
        .collect();

    Ok(Json(json!({ "group_id": group_id, "messages": messages })))
}

pub async fn fetch_blob(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(hash): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let hash = hash.to_lowercase();
    let signer = auth(&headers, "get", &format!("/mls/blobs/{hash}")).await?;

    let (allowed, blob): (bool, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM mls_message_refs r \
                        JOIN mls_group_members m ON m.group_id = r.group_id \
                        WHERE r.ciphertext_hash = $1 AND m.member = $2 AND m.removed_epoch IS NULL), \
                (SELECT ciphertext FROM mls_message_blobs WHERE ciphertext_hash = $1)",
    )
    .bind(&hash)
    .bind(&signer)
    .fetch_one(&state.pool)
    .await?;
    if !allowed {
        return Err(forbidden("not authorized for this blob"));
    }

    match blob {
        Some(b) => Ok(Json(json!({ "hash": hash, "ciphertext": b64(&b) }))),
        None => Err(ApiError::not_found("blob not found")),
    }
}
