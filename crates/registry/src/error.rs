use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("storage error: {0}")]
    Storage(#[from] npm_storage::StorageError),

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for RegistryError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            RegistryError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            RegistryError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            RegistryError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            RegistryError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            RegistryError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            RegistryError::Database(e) => {
                tracing::error!(error = %e, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
            RegistryError::Storage(e) => {
                tracing::error!(error = %e, "storage error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
            RegistryError::Internal(msg) => {
                tracing::error!(error = %msg, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, msg.clone())
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type Result<T> = std::result::Result<T, RegistryError>;
