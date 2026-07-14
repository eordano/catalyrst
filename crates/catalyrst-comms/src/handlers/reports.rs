use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use catalyrst_types::{is_eth_address, limit_or_max};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth_chain::verify_signed_fetch;
use crate::extract::{validate_body, SchemaValidate};
use crate::http::{auth_error, conflict, forbidden, not_found_labeled, unauthorized, ApiError};
use crate::moderator::{authorize_moderator, ModeratorMode};
use crate::ports::player_reports::{
    is_allowed_content_type, CreateReport, EvidenceRequest, EvidenceUploadError, PresignError,
    ReportWriteError, ALLOWED_EVIDENCE_CONTENT_TYPES, MAX_COMMENTS_CHARS, MAX_DESCRIPTION_CHARS,
    MAX_EVIDENCE_BYTES, MAX_EVIDENCE_FILES, MAX_FILENAME_CHARS, MAX_REASON_CHARS,
};
use crate::AppState;

const SCENE_SIGNER: &str = "decentraland-kernel-scene";

pub const PRESIGN_PATH: &str = "/reports/players/presign";
pub const CREATE_PATH: &str = "/reports/players";
pub const LIST_PATH: &str = "/reports";

pub fn evidence_path(report_id: &Uuid, key: &str) -> String {
    format!("/reports/players/{report_id}/evidence/{key}")
}

async fn require_wallet_signer(
    headers: &HeaderMap,
    method: &str,
    path: &str,
) -> Result<String, ApiError> {
    let sf = verify_signed_fetch(headers, method, path, &[])
        .await
        .map_err(|e| auth_error(e.status, e.message))?;
    let meta_signer = sf.metadata.get("signer").and_then(|v| v.as_str());
    if meta_signer == Some(SCENE_SIGNER) {
        return Err(unauthorized(
            "You are not authorized to access this resource",
        ));
    }
    Ok(sf.signer.as_str().to_string())
}

fn is_reason_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REASON_CHARS
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[derive(Debug, Deserialize)]
pub struct PresignFile {
    pub filename: String,
    #[serde(rename = "contentType")]
    pub content_type: String,
    #[serde(rename = "fileSize")]
    pub file_size: i64,
}

#[derive(Debug, Deserialize)]
pub struct PresignBody {
    pub files: Vec<PresignFile>,
}

impl SchemaValidate for PresignBody {
    fn schema_validate(value: &serde_json::Value) -> Result<(), String> {
        let obj = value
            .as_object()
            .ok_or_else(|| "must be an object".to_string())?;
        for key in obj.keys() {
            if key != "files" {
                return Err(format!("additional property not allowed: {key}"));
            }
        }
        let files = obj
            .get("files")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "files must be an array".to_string())?;
        if files.is_empty() {
            return Err("files must not be empty".to_string());
        }
        if files.len() > MAX_EVIDENCE_FILES {
            return Err(format!("at most {MAX_EVIDENCE_FILES} files are allowed"));
        }
        for file in files {
            let item = file
                .as_object()
                .ok_or_else(|| "each file must be an object".to_string())?;
            for key in item.keys() {
                if !matches!(key.as_str(), "filename" | "contentType" | "fileSize") {
                    return Err(format!("additional property not allowed: {key}"));
                }
            }
            match item.get("filename").and_then(|v| v.as_str()) {
                Some(s) if !s.trim().is_empty() && s.chars().count() <= MAX_FILENAME_CHARS => {}
                _ => return Err("filename must be a non-empty string".to_string()),
            }
            match item.get("contentType").and_then(|v| v.as_str()) {
                Some(s) if is_allowed_content_type(s) => {}
                _ => {
                    return Err(format!(
                        "contentType must be one of: {}",
                        ALLOWED_EVIDENCE_CONTENT_TYPES.join(", ")
                    ))
                }
            }
            match item.get("fileSize").and_then(|v| v.as_i64()) {
                Some(n) if n > 0 && n <= MAX_EVIDENCE_BYTES => {}
                _ => {
                    return Err(format!(
                        "fileSize must be a positive integer of at most {MAX_EVIDENCE_BYTES} bytes"
                    ))
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateReportBody {
    #[serde(rename = "reportId")]
    pub report_id: String,
    #[serde(rename = "reportedAddress")]
    pub reported_address: String,
    pub reason: String,
    pub description: String,
    #[serde(rename = "additionalComments")]
    pub additional_comments: Option<String>,
    #[serde(rename = "evidenceKeys")]
    pub evidence_keys: Option<Vec<String>>,
}

impl SchemaValidate for CreateReportBody {
    fn schema_validate(value: &serde_json::Value) -> Result<(), String> {
        let obj = value
            .as_object()
            .ok_or_else(|| "must be an object".to_string())?;
        for key in obj.keys() {
            if !matches!(
                key.as_str(),
                "reportId"
                    | "playerAddress"
                    | "reportedAddress"
                    | "reason"
                    | "description"
                    | "additionalComments"
                    | "confirmAccuracy"
                    | "evidenceKeys"
            ) {
                return Err(format!("additional property not allowed: {key}"));
            }
        }
        match obj.get("reportId").and_then(|v| v.as_str()) {
            Some(s) if Uuid::parse_str(s).is_ok() => {}
            _ => {
                return Err("reportId must be a uuid minted by /reports/players/presign".to_string())
            }
        }
        match obj.get("reportedAddress").and_then(|v| v.as_str()) {
            Some(s) if is_eth_address(s) => {}
            _ => return Err("reportedAddress must be a 0x-prefixed address".to_string()),
        }
        match obj.get("reason").and_then(|v| v.as_str()) {
            Some(s) if is_reason_token(s) => {}
            _ => {
                return Err(format!(
                    "reason must be a lowercase token of at most {MAX_REASON_CHARS} characters"
                ))
            }
        }
        match obj.get("description").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() && s.chars().count() <= MAX_DESCRIPTION_CHARS => {}
            _ => {
                return Err(format!(
                    "description must be a non-empty string of at most {MAX_DESCRIPTION_CHARS} characters"
                ))
            }
        }
        if let Some(comments) = obj.get("additionalComments") {
            match comments.as_str() {
                Some(s) if s.chars().count() <= MAX_COMMENTS_CHARS => {}
                _ => {
                    return Err(format!(
                        "additionalComments must be a string of at most {MAX_COMMENTS_CHARS} characters"
                    ))
                }
            }
        }
        if obj.get("confirmAccuracy").and_then(|v| v.as_bool()) != Some(true) {
            return Err("confirmAccuracy must be true".to_string());
        }
        if let Some(keys) = obj.get("evidenceKeys") {
            let keys = keys
                .as_array()
                .ok_or_else(|| "evidenceKeys must be an array".to_string())?;
            if keys.len() > MAX_EVIDENCE_FILES {
                return Err(format!("at most {MAX_EVIDENCE_FILES} files are allowed"));
            }
            for key in keys {
                match key.as_str() {
                    Some(s) if !s.is_empty() => {}
                    _ => return Err("evidenceKeys must contain non-empty strings".to_string()),
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ReportsQuery {
    pub reported: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

const MAX_LIMIT: i64 = 100;

pub async fn presign_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let reporter = require_wallet_signer(&headers, "post", PRESIGN_PATH).await?;

    let content_type = headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok());
    let body: PresignBody = validate_body(content_type, &body_bytes)?;

    let files: Vec<EvidenceRequest> = body
        .files
        .into_iter()
        .map(|f| EvidenceRequest {
            filename: f.filename,
            content_type: f.content_type.to_ascii_lowercase(),
            file_size: f.file_size,
        })
        .collect();

    let (report_id, slots) = state
        .player_reports
        .create_evidence_slots(&reporter, &files)
        .await
        .map_err(|e| match e {
            PresignError::TooManyPending => ApiError::http(
                429,
                "Too many evidence uploads pending; submit or abandon the reports already started",
            ),
            PresignError::Db(e) => e,
        })?;

    let files: Vec<serde_json::Value> = slots
        .iter()
        .map(|slot| {
            serde_json::json!({
                "key": slot.key,
                "uploadPath": evidence_path(&report_id, &slot.key),
                "contentType": slot.content_type,
                "fileSize": slot.file_size,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "reportId": report_id.to_string(),
        "files": files,
    })))
}

pub async fn upload_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((report_id, key)): Path<(Uuid, String)>,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let path = evidence_path(&report_id, &key);
    let uploader = require_wallet_signer(&headers, "put", &path).await?;

    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !is_allowed_content_type(content_type) {
        return Err(ApiError::http(
            415,
            format!(
                "Content-Type must be one of: {}",
                ALLOWED_EVIDENCE_CONTENT_TYPES.join(", ")
            ),
        ));
    }
    if body_bytes.len() as i64 > MAX_EVIDENCE_BYTES {
        return Err(ApiError::http(
            413,
            format!("evidence must be at most {MAX_EVIDENCE_BYTES} bytes"),
        ));
    }

    let media = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    state
        .player_reports
        .store_evidence(report_id, &key, &uploader, &media, &body_bytes)
        .await
        .map_err(|e| match e {
            EvidenceUploadError::UnknownSlot => not_found_labeled("Unknown evidence slot"),
            EvidenceUploadError::NotOwner => {
                forbidden("signer did not create this report's evidence slot")
            }
            EvidenceUploadError::AlreadyUploaded => conflict("Evidence already uploaded"),
            EvidenceUploadError::ContentTypeMismatch { expected } => {
                ApiError::bad_request(format!("Content-Type must match the presigned {expected}"))
            }
            EvidenceUploadError::SizeMismatch { expected, actual } => ApiError::bad_request(
                format!("evidence size mismatch: presigned {expected} bytes, received {actual}"),
            ),
            EvidenceUploadError::Db(e) => e,
        })?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let reporter = require_wallet_signer(&headers, "post", CREATE_PATH).await?;

    let content_type = headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok());
    let body: CreateReportBody = validate_body(content_type, &body_bytes)?;

    let report_id = Uuid::parse_str(&body.report_id)
        .map_err(|_| ApiError::bad_request("reportId must be a uuid"))?;
    let reported = body.reported_address.to_lowercase();
    if reported == reporter {
        return Err(ApiError::bad_request("You cannot report your own account"));
    }

    let report = state
        .player_reports
        .create_report(CreateReport {
            report_id,
            reporter,
            reported,
            reason: body.reason,
            description: body.description,
            additional_comments: body.additional_comments.filter(|c| !c.trim().is_empty()),
            evidence_keys: body.evidence_keys.unwrap_or_default(),
        })
        .await
        .map_err(|e| match e {
            ReportWriteError::UnknownReport => {
                not_found_labeled("Unknown reportId; call /reports/players/presign first")
            }
            ReportWriteError::NotOwner => forbidden("signer did not start this report"),
            ReportWriteError::UnknownEvidence(key) => {
                ApiError::bad_request(format!("unknown evidence key: {key}"))
            }
            ReportWriteError::EvidenceMissing(key) => {
                ApiError::bad_request(format!("evidence was never uploaded: {key}"))
            }
            ReportWriteError::AlreadySubmitted => conflict("Report already submitted"),
            ReportWriteError::RateLimited => {
                ApiError::http(429, "Too many reports submitted; try again later")
            }
            ReportWriteError::Db(e) => e,
        })?;

    let data = serde_json::to_value(report).unwrap_or(serde_json::Value::Null);
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "data": data })),
    ))
}

pub async fn list_reports(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ReportsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize_moderator(
        &state,
        &headers,
        "get",
        LIST_PATH,
        ModeratorMode::Read,
        None,
    )
    .await?;

    let limit = limit_or_max(q.limit, MAX_LIMIT);
    let offset = q.offset.filter(|o| *o >= 0).unwrap_or(0);
    let reported = q.reported.filter(|a| !a.is_empty());
    let status = q.status.filter(|s| !s.is_empty());
    let total = state
        .player_reports
        .count_reports(reported.as_deref(), status.as_deref())
        .await?;
    let reports = state
        .player_reports
        .list_reports(reported.as_deref(), status.as_deref(), limit, offset)
        .await?;

    let data = serde_json::to_value(reports).unwrap_or(serde_json::Value::Array(vec![]));
    Ok(Json(serde_json::json!({
        "data": data,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

pub async fn get_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(report_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authorize_moderator(
        &state,
        &headers,
        "get",
        &format!("/reports/{report_id}"),
        ModeratorMode::Read,
        None,
    )
    .await?;

    let report = state
        .player_reports
        .get_report(report_id)
        .await?
        .ok_or_else(|| not_found_labeled("Report not found"))?;
    let evidence = state.player_reports.list_evidence(report_id).await?;

    let data = serde_json::to_value(report).unwrap_or(serde_json::Value::Null);
    let evidence = serde_json::to_value(evidence).unwrap_or(serde_json::Value::Array(vec![]));
    Ok(Json(serde_json::json!({
        "data": data,
        "evidence": evidence,
    })))
}

pub async fn download_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((report_id, key)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    authorize_moderator(
        &state,
        &headers,
        "get",
        &evidence_path(&report_id, &key),
        ModeratorMode::Read,
        None,
    )
    .await?;

    let blob = state
        .player_reports
        .load_evidence(report_id, &key)
        .await?
        .ok_or_else(|| not_found_labeled("Evidence not found"))?;

    let mut response = blob.content.into_response();
    let content_type = if is_allowed_content_type(&blob.content_type) {
        blob.content_type
    } else {
        "application/octet-stream".to_string()
    };
    response.headers_mut().insert(
        CONTENT_TYPE,
        content_type
            .parse()
            .unwrap_or_else(|_| "application/octet-stream".parse().unwrap()),
    );
    let disposition = format!("attachment; filename=\"{key}\"");
    if let Ok(value) = disposition.parse() {
        response.headers_mut().insert(CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::SchemaValidate;
    use serde_json::json;

    fn presign_ok() -> serde_json::Value {
        json!({ "files": [{ "filename": "shot.png", "contentType": "image/png", "fileSize": 1024 }] })
    }

    fn create_ok() -> serde_json::Value {
        json!({
            "reportId": "6f1b4f8a-6a1e-4c2f-9b3d-0d9a2c7b1e55",
            "reportedAddress": "0x1111111111111111111111111111111111111111",
            "reason": "harassment",
            "description": "spam in chat",
            "confirmAccuracy": true,
            "evidenceKeys": ["0-shot.png"],
        })
    }

    #[test]
    fn presign_accepts_the_wizard_shape() {
        assert!(PresignBody::schema_validate(&presign_ok()).is_ok());
    }

    #[test]
    fn presign_rejects_empty_oversized_and_disallowed_files() {
        assert!(PresignBody::schema_validate(&json!({ "files": [] })).is_err());

        let mut many = presign_ok();
        let one = many["files"][0].clone();
        many["files"] = json!([
            one.clone(),
            one.clone(),
            one.clone(),
            one.clone(),
            one.clone(),
            one
        ]);
        assert!(PresignBody::schema_validate(&many).is_err());

        let mut big = presign_ok();
        big["files"][0]["fileSize"] = json!(MAX_EVIDENCE_BYTES + 1);
        assert!(PresignBody::schema_validate(&big).is_err());

        let mut script = presign_ok();
        script["files"][0]["contentType"] = json!("application/x-sh");
        assert!(PresignBody::schema_validate(&script).is_err());

        let mut zero = presign_ok();
        zero["files"][0]["fileSize"] = json!(0);
        assert!(PresignBody::schema_validate(&zero).is_err());
    }

    #[test]
    fn create_accepts_the_wizard_shape() {
        assert!(CreateReportBody::schema_validate(&create_ok()).is_ok());
    }

    #[test]
    fn create_body_tolerates_client_supplied_player_address() {
        let mut body = create_ok();
        body["playerAddress"] = json!("0x2222222222222222222222222222222222222222");
        assert!(CreateReportBody::schema_validate(&body).is_ok());

        let parsed: CreateReportBody = serde_json::from_value(body).unwrap();
        assert_eq!(
            parsed.reported_address,
            "0x1111111111111111111111111111111111111111"
        );
    }

    #[test]
    fn create_requires_confirmation_and_a_valid_target() {
        let mut unconfirmed = create_ok();
        unconfirmed["confirmAccuracy"] = json!(false);
        assert!(CreateReportBody::schema_validate(&unconfirmed).is_err());

        let mut missing = create_ok();
        missing.as_object_mut().unwrap().remove("confirmAccuracy");
        assert!(CreateReportBody::schema_validate(&missing).is_err());

        let mut bad_target = create_ok();
        bad_target["reportedAddress"] = json!("not-an-address");
        assert!(CreateReportBody::schema_validate(&bad_target).is_err());

        let mut bad_id = create_ok();
        bad_id["reportId"] = json!("sim-report-abc");
        assert!(CreateReportBody::schema_validate(&bad_id).is_err());
    }

    #[test]
    fn create_caps_free_text_and_constrains_the_reason_token() {
        let mut long = create_ok();
        long["description"] = json!("x".repeat(MAX_DESCRIPTION_CHARS + 1));
        assert!(CreateReportBody::schema_validate(&long).is_err());

        let mut empty = create_ok();
        empty["description"] = json!("   ");
        assert!(CreateReportBody::schema_validate(&empty).is_err());

        let mut comments = create_ok();
        comments["additionalComments"] = json!("y".repeat(MAX_COMMENTS_CHARS + 1));
        assert!(CreateReportBody::schema_validate(&comments).is_err());

        let mut html = create_ok();
        html["reason"] = json!("<script>alert(1)</script>");
        assert!(CreateReportBody::schema_validate(&html).is_err());
    }

    #[test]
    fn evidence_path_matches_the_signed_route() {
        let id = Uuid::parse_str("6f1b4f8a-6a1e-4c2f-9b3d-0d9a2c7b1e55").unwrap();
        assert_eq!(
            evidence_path(&id, "0-shot.png"),
            "/reports/players/6f1b4f8a-6a1e-4c2f-9b3d-0d9a2c7b1e55/evidence/0-shot.png"
        );
    }
}
