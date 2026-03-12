//! Upstream registry proxy for npm-lite.
//!
//! This crate provides the ability to proxy package requests to an upstream
//! npm registry (e.g. `https://registry.npmjs.org`) when the package is not
//! found locally. Supports per-scope routing rules, metadata caching, and
//! tarball caching.

pub mod cache;
pub mod client;
pub mod config;
pub mod error;
pub mod proxy;
pub mod router;

pub use cache::{
    count_cached_packuments, delete_all_cached_packuments, delete_cached_packument,
    get_cached_packument, put_cached_packument, upstream_tarball_s3_key,
};
pub use client::UpstreamClient;
pub use config::UpstreamConfig;
pub use error::UpstreamError;
pub use router::{RouteTarget, resolve as resolve_upstream};
