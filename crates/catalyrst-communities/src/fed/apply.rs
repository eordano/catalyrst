use catalyrst_fed::Signed;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::fed::ids::{community_id_hex, community_uuid_from_hex, signature_hash_hex};
use crate::fed::messages::{
    CommunityBan, CommunityCreate, CommunityDelete, CommunityJoin, CommunityLeave,
    CommunityPlaceRemove, CommunityPlacesAdd, CommunityPost, CommunityPostDelete,
    CommunityPostLike, CommunityPostUnlike, CommunityRequestStatusUpdate, CommunityRole,
    CommunityUnban, CommunityUpdate,
};
use crate::http::ApiError;

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

pub struct AppliedCreate {
    pub community_id: String,
    pub uuid: Uuid,
    pub signature_hash: String,
}

pub async fn apply_create(
    pool: &PgPool,
    signed: &Signed<CommunityCreate>,
    signer: &str,
) -> Result<AppliedCreate, ApiError> {
    let community_id = community_id_hex(signer, &signed.message.name, &signed.nonce);
    let uuid = community_uuid_from_hex(&community_id);
    let sig_hash = signature_hash_hex(&signed.hash());
    let now = now_secs();
    let nonce_hex = hex::encode(signed.nonce);

    let mut tx = pool.begin().await?;

    let existed: Option<(String,)> =
        sqlx::query_as("SELECT community_id FROM communities_local WHERE community_id = $1")
            .bind(&community_id)
            .fetch_optional(&mut *tx)
            .await?;
    if existed.is_some() {
        return Err(ApiError::Http(catalyrst_types::HttpError::new(
            409,
            "community already exists",
        )));
    }

    sqlx::query(
        "INSERT INTO communities_local (community_id, creator, signature, name, description, signed_at, nonce, received_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(&community_id)
    .bind(signer.to_ascii_lowercase())
    .bind(&signed.signature)
    .bind(&signed.message.name)
    .bind(&signed.message.description)
    .bind(signed.signed_at)
    .bind(&nonce_hex)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address, private, active, unlisted, created_at, updated_at) \
         VALUES ($1,$2,$3,$4,$5,TRUE,$6,now(),now())",
    )
    .bind(uuid)
    .bind(&signed.message.name)
    .bind(&signed.message.description)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.message.private)
    .bind(signed.message.unlisted)
    .execute(&mut *tx)
    .await?;

    let owner_payload = json!({
        "type": "CommunityCreate",
        "community_id": community_id,
        "creator": signer.to_ascii_lowercase(),
        "role": "owner",
    });
    sqlx::query(
        "INSERT INTO community_role_log (signature_hash, community_id, signer, target, role, signed_at, message_payload, received_at) \
         VALUES ($1,$2,$3,$4,'owner',$5,$6,$7)",
    )
    .bind(&sig_hash)
    .bind(&community_id)
    .bind(signer.to_ascii_lowercase())
    .bind(signer.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(&owner_payload)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO community_members (community_id, member_address, role, joined_at) \
         VALUES ($1,$2,'owner', now()) ON CONFLICT (community_id, member_address) DO NOTHING",
    )
    .bind(uuid)
    .bind(signer.to_ascii_lowercase())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(AppliedCreate {
        community_id,
        uuid,
        signature_hash: sig_hash,
    })
}

pub async fn apply_update(
    pool: &PgPool,
    signed: &Signed<CommunityUpdate>,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let uuid = community_uuid_from_hex(&signed.message.community_id);

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE communities_local SET \
            name = COALESCE($2, name), \
            description = COALESCE($3, description) \
          WHERE community_id = $1",
    )
    .bind(&signed.message.community_id)
    .bind(signed.message.name.as_deref())
    .bind(signed.message.description.as_deref())
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE communities SET \
            name = COALESCE($2, name), \
            description = COALESCE($3, description), \
            private = COALESCE($4, private), \
            unlisted = COALESCE($5, unlisted), \
            updated_at = now() \
          WHERE id = $1",
    )
    .bind(uuid)
    .bind(signed.message.name.as_deref())
    .bind(signed.message.description.as_deref())
    .bind(signed.message.private)
    .bind(signed.message.unlisted)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(sig_hash)
}

pub async fn apply_delete(
    pool: &PgPool,
    signed: &Signed<CommunityDelete>,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let uuid = community_uuid_from_hex(&signed.message.community_id);
    sqlx::query("UPDATE communities SET active = FALSE, updated_at = now() WHERE id = $1")
        .bind(uuid)
        .execute(pool)
        .await?;
    Ok(sig_hash)
}

pub async fn apply_role(
    pool: &PgPool,
    signed: &Signed<CommunityRole>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let uuid = community_uuid_from_hex(&signed.message.community_id);
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    let role = crate::fed::authority::Role::parse(&signed.message.role)
        .ok_or_else(|| ApiError::Http(catalyrst_types::HttpError::new(400, "invalid role")))?
        .as_str();

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO community_role_log (signature_hash, community_id, signer, target, role, signed_at, message_payload, received_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.community_id)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.message.target.to_ascii_lowercase())
    .bind(role)
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let current: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM community_role_current WHERE community_id = $1 AND member = $2",
    )
    .bind(&signed.message.community_id)
    .bind(signed.message.target.to_ascii_lowercase())
    .fetch_optional(&mut *tx)
    .await?;

    if let Some((role,)) = current {
        if role == "banned" {
            sqlx::query(
                "DELETE FROM community_members WHERE community_id = $1 AND member_address = $2",
            )
            .bind(uuid)
            .bind(signed.message.target.to_ascii_lowercase())
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO community_members (community_id, member_address, role, joined_at) \
                 VALUES ($1, $2, $3, now()) \
                 ON CONFLICT (community_id, member_address) DO UPDATE SET role = EXCLUDED.role",
            )
            .bind(uuid)
            .bind(signed.message.target.to_ascii_lowercase())
            .bind(&role)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(sig_hash)
}

pub async fn apply_join(
    pool: &PgPool,
    signed: &Signed<CommunityJoin>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let uuid = community_uuid_from_hex(&signed.message.community_id);
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO community_role_log (signature_hash, community_id, signer, target, role, signed_at, message_payload, received_at) \
         VALUES ($1,$2,$3,$4,'member',$5,$6,$7) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.community_id)
    .bind(signer.to_ascii_lowercase())
    .bind(signer.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO community_members (community_id, member_address, role, joined_at) \
         VALUES ($1, $2, 'member', now()) \
         ON CONFLICT (community_id, member_address) DO NOTHING",
    )
    .bind(uuid)
    .bind(signer.to_ascii_lowercase())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(sig_hash)
}

pub async fn apply_leave(
    pool: &PgPool,
    signed: &Signed<CommunityLeave>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let uuid = community_uuid_from_hex(&signed.message.community_id);
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO community_role_log (signature_hash, community_id, signer, target, role, signed_at, message_payload, received_at) \
         VALUES ($1,$2,$3,$4,'none',$5,$6,$7) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.community_id)
    .bind(signer.to_ascii_lowercase())
    .bind(signer.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM community_role_current WHERE community_id = $1 AND member = $2")
        .bind(&signed.message.community_id)
        .bind(signer.to_ascii_lowercase())
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM community_members WHERE community_id = $1 AND member_address = $2")
        .bind(uuid)
        .bind(signer.to_ascii_lowercase())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(sig_hash)
}

pub async fn apply_ban(
    pool: &PgPool,
    signed: &Signed<CommunityBan>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let uuid = community_uuid_from_hex(&signed.message.community_id);
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO community_role_log (signature_hash, community_id, signer, target, role, signed_at, message_payload, received_at) \
         VALUES ($1,$2,$3,$4,'banned',$5,$6,$7) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.community_id)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.message.target.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM community_members WHERE community_id = $1 AND member_address = $2")
        .bind(uuid)
        .bind(signed.message.target.to_ascii_lowercase())
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM community_requests \
         WHERE community_id = $1 AND member_address = $2 AND status = 'pending'",
    )
    .bind(uuid)
    .bind(signed.message.target.to_ascii_lowercase())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO community_bans (community_id, banned_address, banned_by, reason, active, banned_at) \
         VALUES ($1,$2,$3,$4,TRUE, now()) \
         ON CONFLICT (community_id, banned_address) DO UPDATE \
           SET active = TRUE, banned_by = EXCLUDED.banned_by, reason = EXCLUDED.reason, banned_at = now(), \
               unbanned_by = NULL, unbanned_at = NULL",
    )
    .bind(uuid)
    .bind(signed.message.target.to_ascii_lowercase())
    .bind(signer.to_ascii_lowercase())
    .bind(signed.message.reason.as_deref())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(sig_hash)
}

pub async fn apply_unban(
    pool: &PgPool,
    signed: &Signed<CommunityUnban>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let uuid = community_uuid_from_hex(&signed.message.community_id);
    let now = now_secs();
    let payload = serde_json::to_value(&signed.message).unwrap_or(json!({}));

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO community_role_log (signature_hash, community_id, signer, target, role, signed_at, message_payload, received_at) \
         VALUES ($1,$2,$3,$4,'none',$5,$6,$7) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.community_id)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.message.target.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(&payload)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM community_role_current WHERE community_id = $1 AND member = $2")
        .bind(&signed.message.community_id)
        .bind(signed.message.target.to_ascii_lowercase())
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE community_bans SET active = FALSE, unbanned_by = $3, unbanned_at = now() \
          WHERE community_id = $1 AND banned_address = $2",
    )
    .bind(uuid)
    .bind(signed.message.target.to_ascii_lowercase())
    .bind(signer.to_ascii_lowercase())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(sig_hash)
}

pub async fn apply_post(
    pool: &PgPool,
    signed: &Signed<CommunityPost>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let uuid = community_uuid_from_hex(&signed.message.community_id);
    let now = now_secs();

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO community_posts_log (signature_hash, community_id, author, content_hash, signed_at, received_at) \
         VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.community_id)
    .bind(signer.to_ascii_lowercase())
    .bind(&signed.message.content_hash)
    .bind(signed.signed_at)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO community_posts (id, community_id, author_address, content, created_at) \
         VALUES (gen_random_uuid(), $1, $2, $3, now())",
    )
    .bind(uuid)
    .bind(signer.to_ascii_lowercase())
    .bind(&signed.message.content_hash)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(sig_hash)
}

pub async fn apply_post_delete(
    pool: &PgPool,
    signed: &Signed<CommunityPostDelete>,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    sqlx::query(
        "UPDATE community_posts_log SET deleted_by_sig = $2 WHERE signature_hash = $1 AND deleted_by_sig IS NULL",
    )
    .bind(&signed.message.post_id)
    .bind(&sig_hash)
    .execute(pool)
    .await?;
    Ok(sig_hash)
}

pub async fn apply_post_like(
    pool: &PgPool,
    signed: &Signed<CommunityPostLike>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let now = now_secs();
    sqlx::query(
        "INSERT INTO community_post_likes_log (signature_hash, post_signature_hash, signer, signed_at, received_at) \
         VALUES ($1,$2,$3,$4,$5) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.post_id)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(sig_hash)
}

pub async fn apply_post_unlike(
    pool: &PgPool,
    signed: &Signed<CommunityPostUnlike>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    sqlx::query(
        "UPDATE community_post_likes_log SET unliked_by_sig = $1 \
          WHERE post_signature_hash = $2 AND signer = $3 AND unliked_by_sig IS NULL",
    )
    .bind(&sig_hash)
    .bind(&signed.message.post_id)
    .bind(signer.to_ascii_lowercase())
    .execute(pool)
    .await?;
    Ok(sig_hash)
}

pub async fn apply_places_add(
    pool: &PgPool,
    signed: &Signed<CommunityPlacesAdd>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let uuid = community_uuid_from_hex(&signed.message.community_id);
    let now = now_secs();

    let mut tx = pool.begin().await?;
    for (i, pid) in signed.message.place_ids.iter().enumerate() {
        let per_sig = format!("{}-add-{}", sig_hash, i);
        sqlx::query(
            "INSERT INTO community_places_log (signature_hash, community_id, place_id, action, signer, signed_at, received_at) \
             VALUES ($1,$2,$3,'add',$4,$5,$6) ON CONFLICT (signature_hash) DO NOTHING",
        )
        .bind(&per_sig)
        .bind(&signed.message.community_id)
        .bind(pid)
        .bind(signer.to_ascii_lowercase())
        .bind(signed.signed_at)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO community_places (id, community_id, added_by, added_at) \
             VALUES ($1,$2,$3, now()) ON CONFLICT (id, community_id) DO NOTHING",
        )
        .bind(pid)
        .bind(uuid)
        .bind(signer.to_ascii_lowercase())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(sig_hash)
}

pub async fn apply_place_remove(
    pool: &PgPool,
    signed: &Signed<CommunityPlaceRemove>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let uuid = community_uuid_from_hex(&signed.message.community_id);
    let now = now_secs();

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO community_places_log (signature_hash, community_id, place_id, action, signer, signed_at, received_at) \
         VALUES ($1,$2,$3,'remove',$4,$5,$6) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.community_id)
    .bind(&signed.message.place_id)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM community_places WHERE id = $1 AND community_id = $2")
        .bind(&signed.message.place_id)
        .bind(uuid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(sig_hash)
}

pub async fn apply_request_status(
    pool: &PgPool,
    signed: &Signed<CommunityRequestStatusUpdate>,
    signer: &str,
) -> Result<String, ApiError> {
    let sig_hash = signature_hash_hex(&signed.hash());
    let now = now_secs();
    sqlx::query(
        "INSERT INTO community_requests_log (signature_hash, community_id, request_id, status, signer, signed_at, received_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (signature_hash) DO NOTHING",
    )
    .bind(&sig_hash)
    .bind(&signed.message.community_id)
    .bind(&signed.message.request_id)
    .bind(&signed.message.status)
    .bind(signer.to_ascii_lowercase())
    .bind(signed.signed_at)
    .bind(now)
    .execute(pool)
    .await?;

    if let Ok(uuid) = Uuid::parse_str(&signed.message.request_id) {
        sqlx::query("UPDATE community_requests SET status = $2, updated_at = now() WHERE id = $1")
            .bind(uuid)
            .bind(&signed.message.status)
            .execute(pool)
            .await?;
    }
    Ok(sig_hash)
}
