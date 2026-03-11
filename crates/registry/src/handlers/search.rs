use axum::{extract::{Query, State}, Json};
use serde::{Deserialize};
use serde_json::{json, Value};
use npm_db::PackageRepo;
use crate::{state::AppState, error::RegistryError};

#[derive(Deserialize)]
pub struct SearchQuery {
    text: Option<String>,
    size: Option<u64>,
    from: Option<u64>,
}

pub async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, RegistryError> {
    let text = query.text.unwrap_or_default();
    let size = query.size.unwrap_or(20).min(250);
    let from = query.from.unwrap_or(0);

    let packages = PackageRepo::list(&state.db, Some(&text), from, size).await
        .map_err(|e| RegistryError::Internal(e.to_string()))?;

    let objects: Vec<Value> = packages.iter().map(|p| {
        json!({
            "package": {
                "name": p.name,
                "scope": p.scope,
                "version": "latest",
                "description": p.description,
                "date": p.updated_at.to_rfc3339(),
            }
        })
    }).collect();

    let total = PackageRepo::count(&state.db).await
        .map_err(|e| RegistryError::Internal(e.to_string()))?;

    Ok(Json(json!({
        "objects": objects,
        "total": total,
        "time": "0ms"
    })))
}
