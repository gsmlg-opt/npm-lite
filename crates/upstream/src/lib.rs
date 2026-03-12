//! Upstream registry proxy for npm-lite.
//!
//! This crate provides the ability to proxy package requests to an upstream
//! npm registry (e.g. `https://registry.npmjs.org`) when the package is not
//! found locally.

pub mod client;
pub mod config;
pub mod error;
pub mod proxy;

pub use client::UpstreamClient;
pub use config::UpstreamConfig;
pub use error::UpstreamError;
