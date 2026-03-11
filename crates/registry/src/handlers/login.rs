//! `PUT /-/user/org.couchdb.user:{username}` – npm login endpoint.
//!
//! The npm CLI sends a PUT request with a JSON body containing the user's
//! credentials.  The user must already exist (created by an admin).  The
//! password is verified and a new token is issued.
//!
//! Response format (npm-compatible):
//! ```json
//! { "ok": true, "token": "<raw-token>" }
//! ```

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use npm_core::auth::{generate_token, hash_token, verify_password};
use npm_entity::tokens;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use npm_entity::users;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    error::{RegistryError, Result},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request body
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub name: Option<String>,
    pub password: String,
    pub email: Option<String>,
    /// npm also sends `_id` and `type` fields; we ignore them.
    #[serde(rename = "_id")]
    pub _id: Option<String>,
    #[serde(rename = "type")]
    pub user_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `PUT /-/user/org.couchdb.user:{username}`
///
/// Authenticates an existing user and issues a new token.  Self-registration
/// is disabled — users must be created by an admin via the admin UI.
pub async fn login_or_adduser(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Json(body): Json<LoginBody>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    // The path parameter looks like `org.couchdb.user:alice`; strip the prefix.
    let username = username
        .strip_prefix("org.couchdb.user:")
        .unwrap_or(&username)
        .to_string();

    if username.is_empty() {
        return Err(RegistryError::BadRequest("username must not be empty".to_string()));
    }

    if body.password.is_empty() {
        return Err(RegistryError::BadRequest("password must not be empty".to_string()));
    }

    // Look up the user.
    let existing_user = users::Entity::find()
        .filter(users::Column::Username.eq(&username))
        .one(&state.db)
        .await?;

    let user = existing_user.ok_or_else(|| {
        RegistryError::Unauthorized("user not found".to_string())
    })?;

    // Verify password.
    let valid = verify_password(&body.password, &user.password_hash)
        .map_err(|e| RegistryError::Internal(e.to_string()))?;

    if !valid {
        return Err(RegistryError::Unauthorized("incorrect password".to_string()));
    }

    let user_id = user.id;
    let user_role = user.role.clone();

    // Generate and store a new token.
    let raw_token = generate_token();
    let token_hash = hash_token(&raw_token);

    let token_now = chrono::Utc::now();
    let new_token = tokens::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        token_hash: Set(token_hash),
        role: Set(user_role),
        name: Set(Some(format!("npm-cli-{}", token_now.timestamp()))),
        created_at: Set(token_now.fixed_offset()),
        revoked_at: Set(None),
    };

    new_token.insert(&state.db).await?;

    tracing::info!(username = %username, "user authenticated, token issued");

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "ok": true,
            "id": format!("org.couchdb.user:{}", username),
            "rev": format!("1-{}", Uuid::new_v4()),
            "token": raw_token,
        })),
    ))
}
