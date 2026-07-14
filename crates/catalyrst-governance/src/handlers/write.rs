use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use crate::handlers::read::ErrorBody;
use crate::snapshot::client::SnapshotError;
use crate::snapshot::templates::ProposalKind;
use crate::snapshot::{SnapshotGate, SubmitError};

pub type WriteState = Arc<SnapshotGate>;

const SIGNED_FETCH_MAX_AGE_SECS: i64 = 5 * 60;

const BID_NOT_A_SNAPSHOT_WRITE: &str = "bid submission is not implemented: a bid does not create a snapshot proposal when it is submitted, it is held unpublished until its tender closes, and this server has no bid store";

#[derive(Serialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "governance/")
)]
pub struct CreatedProposalBody {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub snapshot_space: String,
    pub ipfs: String,
    pub title: String,
    pub pending: bool,
    pub published: bool,
}

pub async fn submit_proposal(
    State(gate): State<WriteState>,
    Path(kind): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(kind) = ProposalKind::from_path(&kind) else {
        return error(
            StatusCode::NOT_FOUND,
            format!("unknown proposal type: {kind}"),
        );
    };

    let path = format!("/proposals/{}", kind.as_path());
    // Authenticate before the credential gate: an anonymous caller must not be able to
    // probe which Snapshot secrets this deployment is missing.
    let author = match catalyrst_crypto::signed_fetch::verify_signed_fetch(
        &headers,
        "post",
        &path,
        SIGNED_FETCH_MAX_AGE_SECS,
    )
    .await
    {
        Ok(signer) => signer,
        Err(e) => {
            tracing::warn!(kind = kind.as_path(), error = %e, "proposal submission rejected: unauthenticated");
            return error(
                StatusCode::UNAUTHORIZED,
                "a signed-fetch identity is required to submit a proposal",
            );
        }
    };

    if matches!(kind, ProposalKind::Bid) {
        return error(StatusCode::NOT_IMPLEMENTED, BID_NOT_A_SNAPSHOT_WRITE);
    }

    let submitter = match gate.as_ref() {
        SnapshotGate::Ready(submitter) => submitter,
        SnapshotGate::Unconfigured(message) => {
            tracing::warn!(
                kind = kind.as_path(),
                "proposal submission refused: {message}"
            );
            return error(StatusCode::SERVICE_UNAVAILABLE, message.clone());
        }
    };

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(e) => {
            return error(StatusCode::BAD_REQUEST, format!("invalid json body: {e}"));
        }
    };

    match submitter.submit(kind, &payload, author.as_str()).await {
        Ok(submission) => (
            StatusCode::CREATED,
            Json(CreatedProposalBody {
                id: submission.id,
                kind: kind.as_path().to_string(),
                snapshot_space: submission.space,
                ipfs: submission.ipfs,
                title: submission.title,
                pending: submission.pending,
                published: true,
            }),
        )
            .into_response(),
        Err(SubmitError::BadRequest(detail)) => error(StatusCode::BAD_REQUEST, detail),
        Err(SubmitError::Upstream(e)) => {
            tracing::error!(kind = kind.as_path(), error = %e, "snapshot proposal submission failed");
            let status = match e {
                SnapshotError::Signing(_) => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::BAD_GATEWAY,
            };
            error(status, e.to_string())
        }
    }
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(ErrorBody {
            error: message.into(),
        }),
    )
        .into_response()
}
