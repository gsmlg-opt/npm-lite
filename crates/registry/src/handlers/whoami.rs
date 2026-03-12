use crate::{auth::AuthUser, error::RegistryError, state::AppState};
use axum::{Json, extract::State};
use serde_json::{Value, json};

pub async fn whoami(_state: State<AppState>, user: AuthUser) -> Result<Json<Value>, RegistryError> {
    Ok(Json(json!({ "username": user.username })))
}
