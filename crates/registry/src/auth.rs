//! Authentication middleware and extractors.
//!
//! - [`AuthUser`] – Axum extractor that validates a Bearer token from the
//!   `Authorization` header, looks it up in the database, and resolves the
//!   associated user and role.
//! - [`AdminUser`] / [`PublishUser`] – convenience extractors that additionally
//!   enforce a minimum required role.

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use npm_core::types::Role;
use npm_entity::{tokens, users};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// AuthUser
// ---------------------------------------------------------------------------

/// Represents a successfully authenticated user, resolved from a Bearer token.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub username: String,
    pub role: Role,
    /// The role attached to the token (may be more restrictive than the user's
    /// own role).
    pub token_role: Role,
    /// Effective role: the more restrictive of `user.role` and `token.role`.
    pub effective_role: Role,
}

impl AuthUser {
    /// Returns `true` when the user's effective role is at least `required`.
    pub fn has_role(&self, required: Role) -> bool {
        role_gte(self.effective_role, required)
    }
}

/// Compare roles: Admin ≥ Publish ≥ Read.
fn role_gte(a: Role, b: Role) -> bool {
    role_weight(a) >= role_weight(b)
}

fn role_weight(r: Role) -> u8 {
    match r {
        Role::Read => 1,
        Role::Publish => 2,
        Role::Admin => 3,
    }
}

fn parse_role(s: &str) -> Role {
    match s {
        "admin" => Role::Admin,
        "publish" => Role::Publish,
        _ => Role::Read,
    }
}

/// Rejection returned when token extraction or validation fails.
pub struct AuthRejection(StatusCode, &'static str);

impl IntoResponse for AuthRejection {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        // Extract the Authorization header.
        let auth_header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthRejection(
                StatusCode::UNAUTHORIZED,
                "missing Authorization header",
            ))?;

        // Must be a Bearer token.
        let raw_token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AuthRejection(
                StatusCode::UNAUTHORIZED,
                "Authorization header must use Bearer scheme",
            ))?
            .trim();

        // Hash the token for DB lookup (tokens are stored hashed).
        let token_hash = npm_core::auth::hash_token(raw_token);

        // Look up the token record.
        let token_model = tokens::Entity::find()
            .filter(tokens::Column::TokenHash.eq(&token_hash))
            .one(&app_state.db)
            .await
            .map_err(|_| AuthRejection(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or(AuthRejection(StatusCode::UNAUTHORIZED, "invalid token"))?;

        // Reject revoked tokens.
        if token_model.revoked_at.is_some() {
            return Err(AuthRejection(StatusCode::UNAUTHORIZED, "token has been revoked"));
        }

        // Resolve the owning user.
        let user_model = users::Entity::find_by_id(token_model.user_id)
            .one(&app_state.db)
            .await
            .map_err(|_| AuthRejection(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or(AuthRejection(StatusCode::UNAUTHORIZED, "user not found"))?;

        let user_role = parse_role(&user_model.role);
        let token_role = parse_role(&token_model.role);

        // Effective role is the more restrictive of user role and token role.
        let effective_role = if role_weight(token_role) < role_weight(user_role) {
            token_role
        } else {
            user_role
        };

        Ok(AuthUser {
            user_id: user_model.id,
            username: user_model.username,
            role: user_role,
            token_role,
            effective_role,
        })
    }
}

// ---------------------------------------------------------------------------
// Role-specific extractors
// ---------------------------------------------------------------------------

/// Convenience: extract and assert Role::Admin.
pub struct AdminUser(pub AuthUser);

impl<S> FromRequestParts<S> for AdminUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;

        if !user.has_role(Role::Admin) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "admin role required" })),
            )
                .into_response());
        }

        Ok(AdminUser(user))
    }
}

/// Convenience: extract and assert Role::Publish (or higher).
pub struct PublishUser(pub AuthUser);

impl<S> FromRequestParts<S> for PublishUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;

        if !user.has_role(Role::Publish) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "publish role required" })),
            )
                .into_response());
        }

        Ok(PublishUser(user))
    }
}

