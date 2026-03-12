use axum::Json;
use serde_json::{Value, json};

pub async fn ping() -> Json<Value> {
    Json(json!({}))
}
