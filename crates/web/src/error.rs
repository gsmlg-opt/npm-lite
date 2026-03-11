use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebError {
    #[error("not found")]
    NotFound,

    #[error("unauthorized")]
    Unauthorized,

    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            WebError::NotFound => (StatusCode::NOT_FOUND, "404 — Not Found".to_string()),
            WebError::Unauthorized => (StatusCode::UNAUTHORIZED, "401 — Unauthorized".to_string()),
            WebError::Database(e) => {
                tracing::error!(error = %e, "database error in web handler");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "500 — Internal Server Error".to_string(),
                )
            }
            WebError::Internal(msg) => {
                tracing::error!(error = %msg, "internal error in web handler");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "500 — Internal Server Error".to_string(),
                )
            }
        };

        let body = crate::templates::layout(
            &message,
            &format!(
                r#"<div class="flex justify-center items-center min-h-[50vh]">
  <div class="text-center">
    <h1 class="text-5xl font-bold text-error mb-4">{status}</h1>
    <p class="text-lg opacity-70">{msg}</p>
    <a href="/admin/" class="btn btn-primary mt-6">Go to Dashboard</a>
  </div>
</div>"#,
                status = status.as_u16(),
                msg = crate::templates::html_escape(&message),
            ),
        );

        (status, Html(body)).into_response()
    }
}

pub type WebResult<T> = std::result::Result<T, WebError>;
