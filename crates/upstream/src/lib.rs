//! Upstream registry proxy for npm-lite.
//!
//! This crate provides the ability to proxy package requests to an upstream
//! npm registry (e.g. `https://registry.npmjs.org`) when the package is not
//! found locally. Supports per-scope routing rules, metadata caching, and
//! tarball caching.

pub mod cache;
pub mod client;
pub mod config;
pub mod db_rules;
pub mod error;
pub mod health;
pub mod integrity;
pub mod proxy;
pub mod router;
pub mod warmup;
pub mod webhook;

pub use cache::{
    CacheStats, cache_stats, count_cached_packuments, delete_all_cached_packuments,
    delete_cached_packument, evict_oldest_cached_packuments, get_cached_packument,
    put_cached_packument, upstream_tarball_s3_key,
};
pub use client::UpstreamClient;
pub use config::UpstreamConfig;
pub use db_rules::{
    RuleInput, UpstreamRule, apply_db_rules, create_rule, delete_rule, get_rule, list_rules,
    update_rule,
};
pub use error::UpstreamError;
pub use health::{CircuitBreaker, UpstreamHealth};
pub use integrity::verify_tarball_integrity;
pub use router::{RouteTarget, resolve as resolve_upstream};
