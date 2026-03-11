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

/// Extension inserted by the admin session middleware so handlers can identify
/// the logged-in admin user without re-querying.
#[derive(Clone, Debug)]
pub struct AdminSession {
    pub user_id: uuid::Uuid,
    pub username: String,
}

pub async fn require_admin_session(
    State(state): State<AppState>,
    mut request: Request,
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
    let admin_user = npm_entity::users::Entity::find()
        .filter(npm_entity::users::Column::Username.eq(&username))
        .filter(npm_entity::users::Column::Role.eq("admin"))
        .one(&state.db)
        .await
        .ok()
        .flatten();

    match admin_user {
        Some(user) => {
            request.extensions_mut().insert(AdminSession {
                user_id: user.id,
                username: user.username,
            });
            next.run(request).await
        }
        None => redirect,
    }
}
