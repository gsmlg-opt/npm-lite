//! Upstream-specific error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UpstreamError {
    /// The upstream returned a 404 for the requested resource.
    #[error("not found on upstream: {0}")]
    NotFound(String),

    /// The upstream returned a 5xx error.
    #[error("upstream server error: {status} for {url}")]
    UpstreamServerError { status: u16, url: String },

    /// The request to the upstream timed out.
    #[error("upstream request timed out: {0}")]
    Timeout(String),

    /// Network or connection error talking to the upstream.
    #[error("upstream request failed: {0}")]
    Request(#[from] reqwest::Error),

    /// The upstream returned an invalid or unparseable packument.
    #[error("invalid upstream response: {0}")]
    InvalidResponse(String),

    /// No upstream is configured.
    #[error("no upstream configured")]
    NoUpstream,

    /// Circuit breaker is open for this upstream (too many recent failures).
    #[error("upstream circuit open (unhealthy): {0}")]
    CircuitOpen(String),
}
