use axum::{Json, extract::State};
use serde_json::{json, Value};
use crate::{auth::AuthUser, error::RegistryError, state::AppState};

pub async fn whoami(
    _state: State<AppState>,
    user: AuthUser,
) -> Result<Json<Value>, RegistryError> {
    Ok(Json(json!({ "username": user.username })))
}
