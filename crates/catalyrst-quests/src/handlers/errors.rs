use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::db::DbError;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub code: u16,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum QuestError {
    #[error("Quest Validation Error: {0}")]
    QuestValidation(String),
    #[error("Cannot modify a quest if you are not the quest creator")]
    NotQuestCreator,
    #[error("Requested Quest cannot be activated because it may be prevoiusly updated and replaced with a new Quest or it may be already active")]
    QuestNotActivable,
    #[error("Requested Quest was previously updated and replaced with a new Quest")]
    QuestIsNotUpdatable,
    #[error("Quest is currently deactivated")]
    QuestIsCurrentlyDeactivated,
    #[error("Cannot reset a Quest Instance if you are not the Quest Creator")]
    ResetQuestInstanceNotAllowed,
    #[error("Not Found")]
    NotFound,
    #[error("Bad Request: the given ID is not valid")]
    NotUuid,
    #[error("Unknown Internal Error")]
    Internal,
    #[error("Unauthorized")]
    Unauthorized,
}

impl QuestError {
    fn status(&self) -> StatusCode {
        match self {
            Self::QuestValidation(_)
            | Self::QuestNotActivable
            | Self::QuestIsNotUpdatable
            | Self::QuestIsCurrentlyDeactivated
            | Self::NotUuid => StatusCode::BAD_REQUEST,
            Self::NotQuestCreator | Self::ResetQuestInstanceNotAllowed => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
        }
    }
}

impl From<DbError> for QuestError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::NotUuid(_) => Self::NotUuid,
            DbError::NotFound => Self::NotFound,
            _ => Self::Internal,
        }
    }
}

impl IntoResponse for QuestError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ErrorResponse {
            code: status.as_u16(),
            message: self.to_string(),
        };
        (status, Json(body)).into_response()
    }
}
