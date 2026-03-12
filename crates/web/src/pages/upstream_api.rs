//! Admin API endpoints for upstream configuration and rules management.
//!
//! - `GET  /admin/api/upstream/config` — get current upstream configuration
//! - `GET  /admin/api/upstream/rules`  — list all routing rules
//! - `POST /admin/api/upstream/rules`  — create a routing rule
//! - `PUT  /admin/api/upstream/rules/{id}` — update a routing rule
//! - `DELETE /admin/api/upstream/rules/{id}` — delete a routing rule
//! - `DELETE /admin/api/upstream/cache` — purge all cached packages
//! - `DELETE /admin/api/upstream/cache/{package}` — purge a specific cached package

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::json;

use crate::state::AppState;

/// `GET /admin/api/upstream/config` — get current upstream configuration.
pub async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    let cache_stats = npm_upstream::cache_stats();
    let cache_count = npm_upstream::count_cached_packuments(&state.db)
        .await
        .unwrap_or(0);

    let config = match &state.upstream {
        Some(client) => {
            let cfg = client.config();
            let health = client.health_status();
            json!({
                "enabled": true,
                "upstream_url": cfg.upstream_url,
                "cache_enabled": cfg.cache_enabled,
                "cache_ttl_secs": cfg.cache_ttl.as_secs(),
                "timeout_secs": cfg.timeout.as_secs(),
                "local_scopes": cfg.local_scopes,
                "scope_rules": cfg.scope_rules,
                "pattern_rules": cfg.pattern_rules.iter().map(|p| {
                    json!({"pattern": p.pattern, "target": p.target})
                }).collect::<Vec<_>>(),
                "cache_stats": {
                    "cached_packages": cache_count,
                    "hits": cache_stats.hits,
                    "misses": cache_stats.misses,
                    "stale_hits": cache_stats.stale_hits,
                },
                "health": health,
            })
        }
        None => {
            json!({
                "enabled": false,
                "upstream_url": null,
                "cache_enabled": false,
                "cache_ttl_secs": 300,
                "timeout_secs": 30,
                "local_scopes": [],
                "scope_rules": {},
                "pattern_rules": [],
                "cache_stats": {
                    "cached_packages": 0,
                    "hits": 0,
                    "misses": 0,
                    "stale_hits": 0,
                },
            })
        }
    };

    Json(config)
}

/// `GET /admin/api/upstream/rules` — list all DB-stored routing rules.
pub async fn list_rules(State(state): State<AppState>) -> impl IntoResponse {
    match npm_upstream::list_rules(&state.db).await {
        Ok(rules) => (StatusCode::OK, Json(json!({"rules": rules}))).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to list upstream rules");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "failed to list rules"})),
            )
                .into_response()
        }
    }
}

/// `POST /admin/api/upstream/rules` — create a routing rule.
pub async fn create_rule(
    State(state): State<AppState>,
    Json(input): Json<npm_upstream::RuleInput>,
) -> impl IntoResponse {
    // Validate rule_type.
    if !["global", "scope", "pattern"].contains(&input.rule_type.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "rule_type must be 'global', 'scope', or 'pattern'"})),
        )
            .into_response();
    }

    // Validate pattern regex if it's a pattern rule.
    if input.rule_type == "pattern"
        && let Err(e) = regex::Regex::new(&input.match_value)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("invalid regex pattern: {}", e)})),
        )
            .into_response();
    }

    match npm_upstream::create_rule(&state.db, input).await {
        Ok(rule) => (StatusCode::CREATED, Json(json!(rule))).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to create upstream rule");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "failed to create rule"})),
            )
                .into_response()
        }
    }
}

/// `PUT /admin/api/upstream/rules/{id}` — update a routing rule.
pub async fn update_rule(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Json(input): Json<npm_upstream::RuleInput>,
) -> impl IntoResponse {
    if !["global", "scope", "pattern"].contains(&input.rule_type.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "rule_type must be 'global', 'scope', or 'pattern'"})),
        )
            .into_response();
    }

    if input.rule_type == "pattern"
        && let Err(e) = regex::Regex::new(&input.match_value)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("invalid regex pattern: {}", e)})),
        )
            .into_response();
    }

    match npm_upstream::update_rule(&state.db, id, input).await {
        Ok(Some(rule)) => Json(json!(rule)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "rule not found"})),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to update upstream rule");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "failed to update rule"})),
            )
                .into_response()
        }
    }
}

/// `DELETE /admin/api/upstream/rules/{id}` — delete a routing rule.
pub async fn delete_rule(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> impl IntoResponse {
    match npm_upstream::delete_rule(&state.db, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "rule not found"})),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to delete upstream rule");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "failed to delete rule"})),
            )
                .into_response()
        }
    }
}

/// `DELETE /admin/api/upstream/cache` — purge all cached packages.
pub async fn purge_all_cache(State(state): State<AppState>) -> impl IntoResponse {
    let deleted = npm_upstream::delete_all_cached_packuments(&state.db)
        .await
        .unwrap_or(0);

    let tarball_deleted = match state.storage.list_objects(Some("upstream/")).await {
        Ok(objects) => {
            let mut count = 0u64;
            for obj in &objects {
                if state.storage.delete(&obj.key).await.is_ok() {
                    count += 1;
                }
            }
            count
        }
        Err(_) => 0,
    };

    Json(json!({
        "metadata_deleted": deleted,
        "tarballs_deleted": tarball_deleted,
    }))
}

/// `DELETE /admin/api/upstream/cache/{package}` — purge a specific cached package.
pub async fn purge_package_cache(
    State(state): State<AppState>,
    Path(package): Path<String>,
) -> impl IntoResponse {
    let deleted = npm_upstream::delete_cached_packument(&state.db, &package)
        .await
        .unwrap_or(false);

    // Also delete any cached tarballs for this package.
    let prefix = format!("upstream/{}/", package);
    let tarball_deleted = match state.storage.list_objects(Some(&prefix)).await {
        Ok(objects) => {
            let mut count = 0u64;
            for obj in &objects {
                if state.storage.delete(&obj.key).await.is_ok() {
                    count += 1;
                }
            }
            count
        }
        Err(_) => 0,
    };

    Json(json!({
        "metadata_deleted": deleted,
        "tarballs_deleted": tarball_deleted,
    }))
}
