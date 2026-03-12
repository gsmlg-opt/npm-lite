pub mod error;
pub mod middleware;
pub mod pages;
pub mod state;
pub mod templates;

pub use state::AppState;

use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{delete, get, post, put},
};

use pages::{
    access::{access_grant, access_page, access_revoke},
    activity::activity_page,
    dashboard::dashboard_page,
    login::{login_page, login_post, logout},
    packages::{
        dist_tag_delete, dist_tag_set, package_detail_page, package_list_page, version_unpublish,
    },
    settings::settings_page,
    teams::{team_add_member, team_create, team_detail_page, team_list_page, team_remove_member},
    tokens::{token_create, token_list_page, token_revoke},
    upstream::{purge_cache, upstream_page},
    upstream_api,
    users::{user_create, user_list_page},
};

/// Build the admin web UI router.
///
/// All routes are mounted relative to the prefix where this router is nested
/// (typically `/admin`). Callers should merge or nest this router at `/admin`.
pub fn web_router(state: AppState) -> Router<AppState> {
    let protected = Router::new()
        // Dashboard
        .route("/", get(dashboard_page))
        // Packages
        .route("/packages", get(package_list_page))
        .route("/packages/{name}", get(package_detail_page))
        .route(
            "/packages/{name}/versions/{version}/unpublish",
            post(version_unpublish),
        )
        .route("/packages/{name}/dist-tags", post(dist_tag_set))
        .route(
            "/packages/{name}/dist-tags/{tag}/delete",
            post(dist_tag_delete),
        )
        // Tokens
        .route("/tokens", get(token_list_page).post(token_create))
        .route("/tokens/{id}/revoke", post(token_revoke))
        // Teams
        .route("/teams", get(team_list_page).post(team_create))
        .route("/teams/{id}", get(team_detail_page))
        .route("/teams/{id}/members", post(team_add_member))
        .route(
            "/teams/{team_id}/members/{member_id}/remove",
            post(team_remove_member),
        )
        // Access Control
        .route("/access", get(access_page).post(access_grant))
        .route("/access/{id}/revoke", post(access_revoke))
        // Users
        .route("/users", get(user_list_page).post(user_create))
        // Settings
        .route("/settings", get(settings_page))
        // Upstream (UI)
        .route("/upstream", get(upstream_page))
        .route("/upstream/purge-cache", post(purge_cache))
        // Upstream API
        .route("/api/upstream/config", get(upstream_api::get_config))
        .route(
            "/api/upstream/rules",
            get(upstream_api::list_rules).post(upstream_api::create_rule),
        )
        .route(
            "/api/upstream/rules/{id}",
            put(upstream_api::update_rule).delete(upstream_api::delete_rule),
        )
        .route(
            "/api/upstream/cache",
            delete(upstream_api::purge_all_cache),
        )
        .route(
            "/api/upstream/cache/{package}",
            delete(upstream_api::purge_package_cache),
        )
        // Activity log
        .route("/activity", get(activity_page))
        .layer(from_fn_with_state(state, middleware::require_admin_session));

    let public = Router::new()
        // Auth
        .route("/login", get(login_page).post(login_post))
        .route("/logout", post(logout));

    protected.merge(public)
}
