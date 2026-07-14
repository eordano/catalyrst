mod authz;
mod execute;
mod form;
mod validate;

pub(crate) use validate::canon_pointer;

use std::time::Duration;

use axum::extract::{Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::upload_limits;
use crate::AppState;

/// Payload cap for a deployment: every file chunk and field value, multipart framing excluded.
pub const MAX_UPLOAD_SIZE_BYTES: usize = 350 * 1024 * 1024;

/// Wire-size cap for the whole multipart body; enforced on the Content-Length precheck and as this route's axum body limit.
pub const MAX_UPLOAD_WIRE_SIZE_BYTES: usize = MAX_UPLOAD_SIZE_BYTES + 10 * 1024 * 1024;

const _: () = assert!(MAX_UPLOAD_WIRE_SIZE_BYTES >= MAX_UPLOAD_SIZE_BYTES);

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
#[serde(rename_all = "camelCase")]
pub struct DeploySuccess {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub creation_timestamp: i64,
    pub message: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
pub struct DeployErrors {
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "worlds/"))]
pub struct DeployRejection {
    pub error: String,
    pub message: String,
}

fn declared_length_exceeds_limit(declared_len: u64) -> bool {
    declared_len > MAX_UPLOAD_WIRE_SIZE_BYTES as u64
}

fn err_response(messages: Vec<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(DeployErrors { errors: messages }),
    )
        .into_response()
}

fn err_one(message: impl Into<String>) -> Response {
    err_response(vec![message.into()])
}

fn forbidden(message: impl Into<String>) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(DeployErrors {
            errors: vec![message.into()],
        }),
    )
        .into_response()
}

fn internal(message: impl Into<String>) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(DeployErrors {
            errors: vec![message.into()],
        }),
    )
        .into_response()
}

#[utoipa::path(
    post,
    path = "/entities",
    tag = "entities",
    request_body = Vec<u8>,
    responses(
        (status = 200, body = DeploySuccess),
        (status = 400, body = DeployErrors),
        (status = 401, body = DeployErrors),
        (status = 403, body = DeployErrors),
        (status = 408, body = DeployRejection),
        (status = 413, body = DeployErrors),
        (status = 500, body = DeployErrors),
        (status = 503, body = DeployRejection)
    )
)]
pub async fn deploy_entity(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Response {
    let declared: Option<u64> = match upload_limits::declared_content_length(&headers) {
        upload_limits::DeclaredContentLength::Invalid => {
            return err_one(upload_limits::INVALID_CONTENT_LENGTH_MESSAGE).into_response();
        }
        upload_limits::DeclaredContentLength::Absent => None,
        upload_limits::DeclaredContentLength::Bytes(n) => Some(n),
    };

    if let Some(len) = declared {
        if declared_length_exceeds_limit(len) {
            return err_one(upload_limits::PAYLOAD_TOO_LARGE_MESSAGE).into_response();
        }
    }

    let _slot = match upload_limits::try_acquire_upload_slot(state.cfg.max_concurrent_uploads) {
        Some(s) => s,
        None => {
            tracing::warn!(
                active = upload_limits::active_uploads(),
                max = state.cfg.max_concurrent_uploads,
                "POST /entities shed: concurrent-upload cap exceeded"
            );
            return upload_limits::shed_response(upload_limits::CONCURRENCY_SHED_MESSAGE);
        }
    };

    let mut bytes_lease = upload_limits::reserve_in_flight();
    let mut files_lease = upload_limits::reserve_in_flight_files();

    let form = match tokio::time::timeout(
        Duration::from_millis(state.cfg.multipart_upload_timeout_ms),
        form::read_deploy_form(
            multipart,
            &mut bytes_lease,
            &mut files_lease,
            state.cfg.max_in_flight_upload_bytes,
            state.cfg.max_in_flight_upload_files,
        ),
    )
    .await
    {
        Ok(Ok(form)) => form,
        Ok(Err(resp)) => return resp,
        Err(_) => {
            tracing::warn!(
                timeout_ms = state.cfg.multipart_upload_timeout_ms,
                "POST /entities: multipart upload timed out"
            );
            return upload_limits::timeout_response(upload_limits::MULTIPART_TIMEOUT_MESSAGE);
        }
    };

    let timeout_ms = state.cfg.deployment_processing_timeout_ms;
    match tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        execute::deploy_entity_inner(state, headers, form),
    )
    .await
    {
        Ok(resp) => resp.into_response(),
        Err(_) => {
            tracing::warn!(
                timeout_ms,
                "POST /entities: deployment processing timed out"
            );
            upload_limits::timeout_response(&format!(
                "Deployment processing exceeded the {timeout_ms}ms deadline."
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_precheck_compares_against_the_wire_cap() {
        assert_eq!(MAX_UPLOAD_WIRE_SIZE_BYTES, 360 * 1024 * 1024);
        assert!(!declared_length_exceeds_limit(
            MAX_UPLOAD_SIZE_BYTES as u64 + 1
        ));
        assert!(!declared_length_exceeds_limit(
            MAX_UPLOAD_WIRE_SIZE_BYTES as u64
        ));
        assert!(declared_length_exceeds_limit(
            MAX_UPLOAD_WIRE_SIZE_BYTES as u64 + 1
        ));
        assert!(declared_length_exceeds_limit(u64::MAX));
        assert!(!declared_length_exceeds_limit(0));
        assert!(!declared_length_exceeds_limit(1024));
    }
}
