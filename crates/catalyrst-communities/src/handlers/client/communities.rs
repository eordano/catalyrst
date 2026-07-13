use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::fed::authority::Role;
use crate::handlers::communities::thumbnail_url;
use crate::handlers::permissions::Permission;
use crate::AppState;

use super::{
    auth, boundary, err, map_api, map_db, parse_multipart, parse_uuid, require_min_role_uuid,
    require_permission_uuid, store_thumbnail, validate_places_ownership,
};

pub async fn create_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let signer = match auth(&headers, "post", "/v1/communities") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let Some(b) = boundary(&headers) else {
        return err(StatusCode::BAD_REQUEST, "expected multipart/form-data");
    };
    let fields = match parse_multipart(b, body).await {
        Ok(f) => f,
        Err(e) => return e,
    };

    let name = fields.name.unwrap_or_default();
    let description = fields.description.unwrap_or_default();
    let privacy = fields.privacy.unwrap_or_else(|| "public".to_string());
    let visibility = fields.visibility.unwrap_or_else(|| "all".to_string());
    let place_ids = fields.place_ids;
    let thumbnail = fields.thumbnail;
    let has_thumbnail = thumbnail.is_some();

    if let Err(e) = crate::validate::validate_name(&name) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::validate::validate_description(&description) {
        return err(StatusCode::BAD_REQUEST, e);
    }

    if let Some(false) = state.profiles.has_owned_name(&signer).await {
        return err(
            StatusCode::UNAUTHORIZED,
            format!("The user {} doesn't have any names", signer),
        );
    }

    if let Err(e) = validate_places_ownership(&state, &place_ids, &signer).await {
        return e;
    }

    let private = privacy == "private";
    let unlisted = visibility == "unlisted";
    let id = Uuid::new_v4();

    let mut tx = match map_db(state.pool.begin().await) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let ins = sqlx::query(
        "INSERT INTO communities (id, name, description, owner_address, private, active, unlisted, created_at, updated_at) \
         VALUES ($1,$2,$3,$4,$5,TRUE,$6,now(),now())",
    )
    .bind(id)
    .bind(&name)
    .bind(&description)
    .bind(&signer)
    .bind(private)
    .bind(unlisted)
    .execute(&mut *tx)
    .await;
    if let Err(e) = ins {
        return map_db::<()>(Err(e)).unwrap_err();
    }
    let memb = sqlx::query(
        "INSERT INTO community_members (community_id, member_address, role, joined_at) \
         VALUES ($1,$2,'owner', now()) ON CONFLICT (community_id, member_address) DO NOTHING",
    )
    .bind(id)
    .bind(&signer)
    .execute(&mut *tx)
    .await;
    if let Err(e) = memb {
        return map_db::<()>(Err(e)).unwrap_err();
    }
    if let Some(bytes) = thumbnail.as_deref() {
        if let Err(e) = store_thumbnail(&mut *tx, &state.content_store, id, bytes).await {
            return e;
        }
    }
    for pid in &place_ids {
        let _ = sqlx::query(
            "INSERT INTO community_places (id, community_id, added_by, added_at) \
             VALUES ($1,$2,$3, now()) ON CONFLICT (id, community_id) DO NOTHING",
        )
        .bind(pid)
        .bind(id)
        .bind(&signer)
        .execute(&mut *tx)
        .await;
    }
    if let Err(e) = map_db(tx.commit().await) {
        return e;
    }

    let privacy_out = if private { "private" } else { "public" };
    let visibility_out = if unlisted { "unlisted" } else { "all" };
    let thumb = if has_thumbnail {
        thumbnail_url(&state.cdn_url, &id.to_string())
    } else {
        "N/A".to_string()
    };
    let data = json!({
        "id": id,
        "name": name,
        "description": description,
        "ownerAddress": signer,
        "privacy": privacy_out,
        "visibility": visibility_out,
        "thumbnailUrl": thumb,
        "active": true,
        "role": "owner",
        "membersCount": 1,
    });
    (
        StatusCode::CREATED,
        Json(json!({ "message": "Community created successfully", "data": data })),
    )
        .into_response()
}

pub async fn update_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}", id);
    let signer = match auth(&headers, "put", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };

    if let Err(e) = require_permission_uuid(
        &state,
        uuid,
        &signer,
        Permission::EditInfo,
        "edit the community",
    )
    .await
    {
        return e;
    }
    let Some(b) = boundary(&headers) else {
        return err(StatusCode::BAD_REQUEST, "expected multipart/form-data");
    };
    let fields = match parse_multipart(b, body).await {
        Ok(f) => f,
        Err(e) => return e,
    };
    let name = fields.name;
    let description = fields.description;
    if let Err(e) = crate::validate::validate_name_opt(name.as_deref()) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::validate::validate_description_opt(description.as_deref()) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    let privacy: Option<bool> = fields.privacy.map(|p| p == "private");
    let visibility: Option<bool> = fields.visibility.map(|v| v == "unlisted");
    let thumbnail = fields.thumbnail;

    let upd = sqlx::query(
        "UPDATE communities SET \
            name = COALESCE($2, name), \
            description = COALESCE($3, description), \
            private = COALESCE($4, private), \
            unlisted = COALESCE($5, unlisted), \
            updated_at = now() \
          WHERE id = $1",
    )
    .bind(uuid)
    .bind(name.as_deref())
    .bind(description.as_deref())
    .bind(privacy)
    .bind(visibility)
    .execute(&state.pool)
    .await;
    if let Err(e) = map_db(upd) {
        return e;
    }
    if let Some(bytes) = thumbnail.as_deref() {
        if let Err(e) = store_thumbnail(&state.pool, &state.content_store, uuid, bytes).await {
            return e;
        }
    }

    let data = match state.communities.get_by_id(uuid, Some(&signer)).await {
        Ok(Some(mut obj)) => {
            let has_thumb = obj
                .as_object_mut()
                .and_then(|m| m.remove("_hasThumbnail"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if let Some(m) = obj.as_object_mut() {
                let thumb = if has_thumb {
                    thumbnail_url(&state.cdn_url, &uuid.to_string())
                } else {
                    "N/A".to_string()
                };
                m.insert("thumbnailUrl".to_string(), serde_json::Value::String(thumb));
            }
            obj
        }
        Ok(None) => return err(StatusCode::NOT_FOUND, "Community not found"),
        Err(e) => return map_api(e),
    };
    (StatusCode::OK, Json(json!({ "data": data }))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct PatchBody {
    #[serde(rename = "editorsChoice", default)]
    pub editors_choice: Option<bool>,
}

pub async fn update_community_partially(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}", id);
    let signer = match auth(&headers, "patch", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };
    if let Err(e) = require_min_role_uuid(&state, uuid, &signer, Role::Owner).await {
        return e;
    }
    let parsed: PatchBody = serde_json::from_slice(&body).unwrap_or(PatchBody {
        editors_choice: None,
    });
    if let Some(ec) = parsed.editors_choice {
        let upd = sqlx::query(
            "UPDATE communities SET editors_choice = $2, updated_at = now() WHERE id = $1",
        )
        .bind(uuid)
        .bind(ec)
        .execute(&state.pool)
        .await;
        if let Err(e) = map_db(upd) {
            return e;
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn delete_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let uuid = match parse_uuid(&id) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let path = format!("/v1/communities/{}", id);
    let signer = match auth(&headers, "delete", &path) {
        Ok(s) => s,
        Err(e) => return e,
    };

    if let Err(e) = require_permission_uuid(
        &state,
        uuid,
        &signer,
        Permission::DeleteCommunity,
        "delete the community",
    )
    .await
    {
        return e;
    }
    let upd =
        sqlx::query("UPDATE communities SET active = FALSE, updated_at = now() WHERE id = $1")
            .bind(uuid)
            .execute(&state.pool)
            .await;
    if let Err(e) = map_db(upd) {
        return e;
    }
    StatusCode::NO_CONTENT.into_response()
}
