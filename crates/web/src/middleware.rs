use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::state::AppState;

/// Extracts the `admin_user` cookie value from the Cookie header.
fn extract_admin_cookie(cookie_header: &str) -> Option<&str> {
    for part in cookie_header.split(';') {
        let trimmed = part.trim();
        if let Some(value) = trimmed.strip_prefix("admin_user=") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

pub async fn require_admin_session(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let redirect = Redirect::to("/admin/login").into_response();

    let username = match request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(extract_admin_cookie)
    {
        Some(u) => u.to_string(),
        None => return redirect,
    };

    // Validate that the username corresponds to an actual admin user in the DB.
    let is_admin = npm_entity::users::Entity::find()
        .filter(npm_entity::users::Column::Username.eq(&username))
        .filter(npm_entity::users::Column::Role.eq("admin"))
        .one(&state.db)
        .await
        .ok()
        .flatten()
        .is_some();

    if !is_admin {
        return redirect;
    }

    next.run(request).await
}
