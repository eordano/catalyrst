use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::rest::community_membership_authority::CommunityMembershipTier;
use crate::rest::handlers::communities::thumbnail_url;
use crate::rest::handlers::permissions::Permission;
use crate::rest::AppState;

use super::{
    auth, boundary, err, map_api, map_db, parse_multipart, parse_uuid, store_thumbnail,
    validate_places_ownership, validate_thumbnail_field, ClientCommunityWriteAuthority,
};

// Cast-then-compare resolved any value the enum does not name to the permissive side -- on update
// a typo could flip a private community public (upstream #487). Refusing is the only reading that
// cannot silently widen access.
fn parse_enum_field(value: String, allowed: [&str; 2], field: &str) -> Result<String, Response> {
    let trimmed = value.trim().to_string();
    if allowed.contains(&trimmed.as_str()) {
        return Ok(trimmed);
    }
    Err(err(
        StatusCode::BAD_REQUEST,
        format!(
            "Invalid {}: {}. Valid values are {}",
            field,
            if trimmed.is_empty() {
                "(empty)"
            } else {
                trimmed.as_str()
            },
            allowed.join(", ")
        ),
    ))
}

fn parse_privacy(value: String) -> Result<String, Response> {
    parse_enum_field(value, ["public", "private"], "privacy")
}

fn parse_visibility(value: String) -> Result<String, Response> {
    parse_enum_field(value, ["all", "unlisted"], "visibility")
}

pub async fn create_community(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let signer = match auth(&headers, "post", "/v1/communities").await {
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
    let privacy = match fields.privacy {
        Some(p) => match parse_privacy(p) {
            Ok(p) => p,
            Err(e) => return e,
        },
        None => "public".to_string(),
    };
    let visibility = match fields.visibility {
        Some(v) => match parse_visibility(v) {
            Ok(v) => v,
            Err(e) => return e,
        },
        None => "all".to_string(),
    };
    let place_ids = fields.place_ids;
    let thumbnail = fields.thumbnail;
    let has_thumbnail = thumbnail.is_some();

    // Validate the thumbnail bytes (size bounds + magic-byte signature) before any DB write, so
    // an arbitrary blob is rejected with a 400 rather than stored and served as a fake image.
    if let Some(bytes) = thumbnail.as_deref() {
        if let Err(e) = validate_thumbnail_field(bytes) {
            return e;
        }
    }

    if let Err(e) = crate::rest::validate::validate_name(&name) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::rest::validate::check_restricted_name(&name, &state.restricted_names) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::rest::validate::validate_description(&description) {
        return err(StatusCode::BAD_REQUEST, e);
    }

    if let Some(false) = state.profiles.has_owned_name(signer.as_str()).await {
        return err(
            StatusCode::UNAUTHORIZED,
            format!("The user {} doesn't have any names", signer),
        );
    }

    if let Err(e) = validate_places_ownership(&state, &place_ids, signer.as_str()).await {
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
    .bind(signer.as_str())
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
    .bind(signer.as_str())
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
        let place = sqlx::query(
            "INSERT INTO community_places (id, community_id, added_by, added_at) \
             VALUES ($1,$2,$3, now()) ON CONFLICT (id, community_id) DO NOTHING",
        )
        .bind(pid)
        .bind(id)
        .bind(signer.as_str())
        .execute(&mut *tx)
        .await;
        if let Err(e) = place {
            return map_db::<()>(Err(e)).unwrap_err();
        }
    }
    if let Err(e) = map_db(tx.commit().await) {
        return e;
    }

    let privacy_out = if private { "private" } else { "public" };
    let visibility_out = if unlisted { "unlisted" } else { "all" };
    let mut data = json!({
        "id": id,
        "name": name,
        "description": description,
        "ownerAddress": signer.as_str(),
        "privacy": privacy_out,
        "visibility": visibility_out,
        "active": true,
        "role": "owner",
        "membersCount": 1,
    });
    if has_thumbnail {
        if let Some(m) = data.as_object_mut() {
            m.insert(
                "thumbnailUrl".to_string(),
                serde_json::Value::String(thumbnail_url(&state.cdn_url, &id.to_string())),
            );
        }
    }
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
    let signer = match auth(&headers, "put", &path).await {
        Ok(s) => s,
        Err(e) => return e,
    };

    if let Err(e) = ClientCommunityWriteAuthority::resolve_requiring_capability(
        &state,
        uuid,
        signer.as_str(),
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
    if let Err(e) = crate::rest::validate::validate_name_opt(name.as_deref()) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) =
        crate::rest::validate::check_restricted_name_opt(name.as_deref(), &state.restricted_names)
    {
        return err(StatusCode::BAD_REQUEST, e);
    }
    if let Err(e) = crate::rest::validate::validate_description_opt(description.as_deref()) {
        return err(StatusCode::BAD_REQUEST, e);
    }
    let privacy: Option<bool> = match fields.privacy {
        Some(p) => match parse_privacy(p) {
            Ok(p) => Some(p == "private"),
            Err(e) => return e,
        },
        None => None,
    };
    let visibility: Option<bool> = match fields.visibility {
        Some(v) => match parse_visibility(v) {
            Ok(v) => Some(v == "unlisted"),
            Err(e) => return e,
        },
        None => None,
    };
    let thumbnail = fields.thumbnail;

    // Reject a non-image / out-of-bounds thumbnail before the DB write (port of #444).
    if let Some(bytes) = thumbnail.as_deref() {
        if let Err(e) = validate_thumbnail_field(bytes) {
            return e;
        }
    }

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

    let data = match state
        .communities
        .get_by_id(uuid, Some(signer.as_str()))
        .await
    {
        Ok(Some(mut obj)) => {
            if obj.has_thumbnail {
                obj.thumbnail_url = Some(thumbnail_url(&state.cdn_url, &uuid.to_string()));
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
    let signer = match auth(&headers, "patch", &path).await {
        Ok(s) => s,
        Err(e) => return e,
    };
    if let Err(e) = ClientCommunityWriteAuthority::resolve_requiring_at_least(
        &state,
        uuid,
        signer.as_str(),
        CommunityMembershipTier::OwnerOfThisCommunity,
    )
    .await
    {
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
    let signer = match auth(&headers, "delete", &path).await {
        Ok(s) => s,
        Err(e) => return e,
    };

    if let Err(e) = ClientCommunityWriteAuthority::resolve_requiring_capability(
        &state,
        uuid,
        signer.as_str(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_fields_refuse_values_the_enum_does_not_name() {
        assert_eq!(parse_privacy("private".into()).unwrap(), "private");
        assert_eq!(parse_privacy(" public ".into()).unwrap(), "public");
        assert_eq!(parse_visibility("unlisted".into()).unwrap(), "unlisted");
        assert_eq!(parse_visibility("all".into()).unwrap(), "all");

        for bad in ["privte", "", "Private", "invalid"] {
            let e = parse_privacy(bad.into()).unwrap_err();
            assert_eq!(e.status(), StatusCode::BAD_REQUEST, "privacy {bad:?}");
        }
        for bad in ["unlsited", "", "hidden"] {
            let e = parse_visibility(bad.into()).unwrap_err();
            assert_eq!(e.status(), StatusCode::BAD_REQUEST, "visibility {bad:?}");
        }
    }

    // Anchored above the parser: the multipart layer used to lowercase these two fields, so the
    // validator saw `private`/`all` and the miscased originals could never be refused.
    #[tokio::test]
    async fn multipart_hands_the_validator_the_raw_case_so_miscased_values_are_refused() {
        let boundary = "XBOUNDARY";
        let body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"privacy\"\r\n\r\nPrivate\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"visibility\"\r\n\r\nALL\r\n\
             --{b}--\r\n",
            b = boundary
        );
        let fields = super::parse_multipart(boundary.to_string(), Bytes::from(body))
            .await
            .expect("well-formed multipart");
        assert_eq!(fields.privacy.as_deref(), Some("Private"));
        assert_eq!(fields.visibility.as_deref(), Some("ALL"));

        let e = parse_privacy(fields.privacy.unwrap()).unwrap_err();
        assert_eq!(e.status(), StatusCode::BAD_REQUEST);
        let e = parse_visibility(fields.visibility.unwrap()).unwrap_err();
        assert_eq!(e.status(), StatusCode::BAD_REQUEST);
    }
}
