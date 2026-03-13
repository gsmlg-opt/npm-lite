//! `npm-registry` – npm-compatible HTTP API handlers.
//!
//! This crate provides the Axum [`Router`] that implements the npm registry
//! wire protocol.  Compose it into a larger application via
//! [`registry_router`].
//!
//! # Example
//!
//! ```rust,ignore
//! use npm_registry::{registry_router, AppState, RegistryConfig};
//! use std::sync::Arc;
//!
//! let state = AppState {
//!     db: db_connection,
//!     storage: Arc::new(s3_storage),
//!     config: RegistryConfig {
//!         registry_url: "https://registry.example.com".to_string(),
//!     },
//! };
//!
//! let app = registry_router().with_state(state);
//! ```

pub mod auth;
pub mod error;
pub mod handlers;
pub mod state;

pub use state::{AppState, RegistryConfig};

use axum::{
    Router,
    routing::{delete, get, put},
};

use handlers::{
    dist_tags::{delete_dist_tag, list_dist_tags, set_dist_tag},
    login::login_or_adduser,
    packument::{get_packument, get_scoped_packument},
    ping::ping,
    publish::{publish_package, publish_scoped_package},
    search::search,
    tarball::{get_scoped_tarball, get_tarball},
    unpublish::{unpublish_package, unpublish_version},
    version::{get_scoped_version, get_version},
    whoami::whoami,
};

/// Build and return the full npm-registry [`Router`].
///
/// The returned router is parameterised by [`AppState`]; call
/// `.with_state(state)` to produce a concrete `Router<()>` that can be served
/// directly or nested inside a parent router.
pub fn registry_router() -> Router<AppState> {
    Router::new()
        // ── Health / discovery ────────────────────────────────────────────
        //
        // GET /-/ping    – liveness check
        // GET /-/v1/search – package search
        // GET /-/whoami  – return authenticated user's username
        .route("/-/ping", get(ping))
        .route("/-/v1/search", get(search))
        .route("/-/whoami", get(whoami))
        // ── Login / user management ─────────────────────────────────────
        //
        // npm login / npm adduser
        //   PUT /-/user/org.couchdb.user:{username}
        .route("/-/user/{username}", put(login_or_adduser))
        // ── Dist-tags ────────────────────────────────────────────────────
        //
        // List:   GET  /-/package/{pkg}/dist-tags
        // Set:    PUT  /-/package/{pkg}/dist-tags/{tag}
        // Delete: DELETE /-/package/{pkg}/dist-tags/{tag}
        .route("/-/package/{package}/dist-tags", get(list_dist_tags))
        .route(
            "/-/package/{package}/dist-tags/{tag}",
            put(set_dist_tag).delete(delete_dist_tag),
        )
        // ── Admin unpublish ───────────────────────────────────────────────
        //
        // Soft-delete a single version or all versions of a package.
        .route("/-/admin/package/{package}", delete(unpublish_package))
        .route(
            "/-/admin/package/{package}/{version}",
            delete(unpublish_version),
        )
        // ── Scoped packages (@scope/name) ─────────────────────────────────
        //
        // Tarball download must come before version/packument because the
        // path segment `{name}/-/{filename}` would otherwise be ambiguous
        // with `{name}/{version}`.
        //
        // GET  /@{scope}/{name}/-/{filename}   – tarball
        // GET  /@{scope}/{name}/{version}       – version metadata
        // GET  /@{scope}/{name}                 – packument
        // PUT  /@{scope}/{name}                 – publish
        .route("/@{scope}/{name}/-/{filename}", get(get_scoped_tarball))
        .route("/@{scope}/{name}/{version}", get(get_scoped_version))
        .route(
            "/@{scope}/{name}",
            get(get_scoped_packument).put(publish_scoped_package),
        )
        // ── Plain (non-scoped) packages ───────────────────────────────────
        //
        // GET  /{package}/-/{filename}   – tarball
        // GET  /{package}/{version}       – version metadata
        // GET  /{package}                 – packument
        // PUT  /{package}                 – publish
        .route("/{package}/-/{filename}", get(get_tarball))
        .route("/{package}/{version}", get(get_version))
        .route("/{package}", get(get_packument).put(publish_package))
}
