pub mod error;
pub mod middleware;
pub mod pages;
pub mod state;
pub mod templates;

pub use state::AppState;

use axum::{
    middleware::from_fn_with_state,
    routing::{get, post},
    Router,
};

use pages::{
    activity::activity_page,
    dashboard::dashboard_page,
    login::{login_page, login_post, logout},
    packages::{package_detail_page, package_list_page},
    teams::{team_add_member, team_create, team_detail_page, team_list_page, team_remove_member},
    tokens::{token_create, token_list_page, token_revoke},
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
        // Activity log
        .route("/activity", get(activity_page))
        .layer(from_fn_with_state(state, middleware::require_admin_session));

    let public = Router::new()
        // Auth
        .route("/login", get(login_page).post(login_post))
        .route("/logout", post(logout));

    protected.merge(public)
}
