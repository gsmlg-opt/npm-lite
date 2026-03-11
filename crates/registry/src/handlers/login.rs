//! `PUT /-/user/org.couchdb.user:{username}` – npm login / adduser endpoint.
//!
//! The npm CLI sends a PUT request with a JSON body containing the user's
//! credentials.  If the user already exists their password is verified and a
//! new token is issued.  If the user is new they are created and a token is
//! returned.
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
use npm_core::auth::{generate_token, hash_password, hash_token, verify_password};
use npm_entity::{tokens, users};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
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
/// Handles both `npm login` (existing user) and `npm adduser` (new user).
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

    let user_id = match existing_user {
        Some(user) => {
            // Existing user: verify password.
            let valid = verify_password(&body.password, &user.password_hash)
                .map_err(|e| RegistryError::Internal(e.to_string()))?;

            if !valid {
                return Err(RegistryError::Unauthorized("incorrect password".to_string()));
            }

            user.id
        }
        None => {
            // New user: create account.
            let email = body.email.unwrap_or_else(|| format!("{}@example.com", username));

            let password_hash = hash_password(&body.password)
                .map_err(|e| RegistryError::Internal(e.to_string()))?;

            let now = chrono::Utc::now().fixed_offset();
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                username: Set(username.clone()),
                password_hash: Set(password_hash),
                email: Set(email),
                role: Set("publish".to_string()),
                created_at: Set(now),
                updated_at: Set(now),
            };

            let inserted = new_user.insert(&state.db).await?;
            inserted.id
        }
    };

    // Generate and store a new token.
    let raw_token = generate_token();
    let token_hash = hash_token(&raw_token);

    let token_now = chrono::Utc::now();
    let new_token = tokens::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        token_hash: Set(token_hash),
        role: Set("publish".to_string()),
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
