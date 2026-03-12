//! Upstream health tracking and circuit breaker.
//!
//! Implements a simple circuit breaker pattern:
//! - **Closed** (healthy): requests pass through normally
//! - **Open** (unhealthy): requests are short-circuited for a cooldown period
//! - **Half-Open**: after cooldown, one probe request is allowed through
//!
//! The circuit opens after `failure_threshold` consecutive failures and
//! stays open for `cooldown_secs` before allowing a probe request.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Circuit breaker state for a single upstream.
#[derive(Debug, Clone, Default)]
struct CircuitState {
    consecutive_failures: u32,
    last_failure: Option<Instant>,
    is_open: bool,
}

/// Circuit breaker manager for all upstream registries.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    states: Arc<Mutex<HashMap<String, CircuitState>>>,
    failure_threshold: u32,
    cooldown: Duration,
}

/// Health status for an upstream.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpstreamHealth {
    pub url: String,
    pub status: String,
    pub consecutive_failures: u32,
    pub last_failure_secs_ago: Option<u64>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given thresholds.
    pub fn new(failure_threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            states: Arc::new(Mutex::new(HashMap::new())),
            failure_threshold,
            cooldown: Duration::from_secs(cooldown_secs),
        }
    }

    /// Check if the given upstream is healthy (circuit closed or half-open).
    ///
    /// Returns `true` if requests should be allowed through.
    pub fn is_healthy(&self, upstream_url: &str) -> bool {
        let normalized = upstream_url.trim_end_matches('/');
        let mut states = self.states.lock().unwrap();
        let state = states.entry(normalized.to_string()).or_default();

        if !state.is_open {
            return true;
        }

        // Check if cooldown has elapsed (half-open).
        if let Some(last_failure) = state.last_failure
            && last_failure.elapsed() >= self.cooldown
        {
            tracing::debug!(upstream = %normalized, "circuit half-open, allowing probe request");
            return true;
        }

        tracing::debug!(
            upstream = %normalized,
            failures = state.consecutive_failures,
            "circuit open, short-circuiting upstream request"
        );
        false
    }

    /// Record a successful request to the given upstream.
    pub fn record_success(&self, upstream_url: &str) {
        let normalized = upstream_url.trim_end_matches('/');
        let mut states = self.states.lock().unwrap();
        let state = states.entry(normalized.to_string()).or_default();

        if state.is_open {
            tracing::info!(upstream = %normalized, "circuit breaker closed (upstream recovered)");
        }

        state.consecutive_failures = 0;
        state.is_open = false;
    }

    /// Record a failed request to the given upstream.
    pub fn record_failure(&self, upstream_url: &str) {
        let normalized = upstream_url.trim_end_matches('/');
        let mut states = self.states.lock().unwrap();
        let state = states.entry(normalized.to_string()).or_default();

        state.consecutive_failures += 1;
        state.last_failure = Some(Instant::now());

        if state.consecutive_failures >= self.failure_threshold && !state.is_open {
            state.is_open = true;
            tracing::warn!(
                upstream = %normalized,
                failures = state.consecutive_failures,
                threshold = self.failure_threshold,
                cooldown_secs = self.cooldown.as_secs(),
                "circuit breaker opened (upstream unhealthy)"
            );
            crate::webhook::notify_circuit_open(normalized);
        }
    }

    /// Get health status for all tracked upstreams.
    pub fn health_status(&self) -> Vec<UpstreamHealth> {
        let states = self.states.lock().unwrap();
        states
            .iter()
            .map(|(url, state)| UpstreamHealth {
                url: url.clone(),
                status: if state.is_open {
                    "open".to_string()
                } else {
                    "closed".to_string()
                },
                consecutive_failures: state.consecutive_failures,
                last_failure_secs_ago: state.last_failure.map(|t| t.elapsed().as_secs()),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_upstream_is_healthy() {
        let cb = CircuitBreaker::new(3, 30);
        assert!(cb.is_healthy("https://registry.npmjs.org"));
    }

    #[test]
    fn opens_after_threshold() {
        let cb = CircuitBreaker::new(3, 30);
        let url = "https://registry.npmjs.org";

        cb.record_failure(url);
        assert!(cb.is_healthy(url));

        cb.record_failure(url);
        assert!(cb.is_healthy(url));

        cb.record_failure(url);
        // Circuit should now be open.
        assert!(!cb.is_healthy(url));
    }

    #[test]
    fn closes_on_success() {
        let cb = CircuitBreaker::new(2, 30);
        let url = "https://registry.npmjs.org";

        cb.record_failure(url);
        cb.record_failure(url);
        assert!(!cb.is_healthy(url));

        cb.record_success(url);
        assert!(cb.is_healthy(url));
    }

    #[test]
    fn success_resets_failure_count() {
        let cb = CircuitBreaker::new(3, 30);
        let url = "https://example.com";

        cb.record_failure(url);
        cb.record_failure(url);
        cb.record_success(url);

        // After success, failure count resets, so one more failure shouldn't trip.
        cb.record_failure(url);
        assert!(cb.is_healthy(url));
    }

    #[test]
    fn half_open_after_cooldown() {
        let cb = CircuitBreaker::new(2, 0); // 0s cooldown for testing
        let url = "https://example.com";

        cb.record_failure(url);
        cb.record_failure(url);

        // Circuit is open, but cooldown is 0s so it should be half-open immediately.
        assert!(cb.is_healthy(url));
    }

    #[test]
    fn health_status_reports_correctly() {
        let cb = CircuitBreaker::new(2, 30);
        let url = "https://registry.npmjs.org";

        cb.record_failure(url);
        cb.record_failure(url);

        let status = cb.health_status();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].status, "open");
        assert_eq!(status[0].consecutive_failures, 2);
    }
}
