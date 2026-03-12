//! Webhook notifications for upstream failures.
//!
//! When configured via `UPSTREAM_WEBHOOK_URL`, sends POST requests with JSON
//! payloads on upstream errors (circuit open, 5xx, timeouts).

use reqwest::Client;
use serde::Serialize;
use std::sync::OnceLock;
use tracing::{debug, warn};

static WEBHOOK_URL: OnceLock<Option<String>> = OnceLock::new();
static WEBHOOK_CLIENT: OnceLock<Client> = OnceLock::new();

/// Initialise the webhook system from environment variables.
fn webhook_url() -> &'static Option<String> {
    WEBHOOK_URL.get_or_init(|| {
        std::env::var("UPSTREAM_WEBHOOK_URL")
            .ok()
            .filter(|s| !s.is_empty())
    })
}

fn webhook_client() -> &'static Client {
    WEBHOOK_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default()
    })
}

/// Webhook event payload.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookEvent {
    pub event_type: String,
    pub upstream_url: String,
    pub message: String,
    pub timestamp: String,
}

/// Fire a webhook notification (best-effort, non-blocking).
///
/// If `UPSTREAM_WEBHOOK_URL` is not set, this is a no-op.
pub fn notify(event_type: &str, upstream_url: &str, message: &str) {
    let Some(url) = webhook_url().as_ref() else {
        return;
    };

    let event = WebhookEvent {
        event_type: event_type.to_string(),
        upstream_url: upstream_url.to_string(),
        message: message.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let url = url.clone();
    let client = webhook_client().clone();

    // Spawn fire-and-forget task.
    tokio::spawn(async move {
        debug!(url = %url, event_type = %event.event_type, "sending upstream webhook");
        match client
            .post(&url)
            .json(&event)
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    warn!(
                        url = %url,
                        status = resp.status().as_u16(),
                        "upstream webhook returned non-2xx"
                    );
                }
            }
            Err(e) => {
                warn!(url = %url, error = %e, "failed to send upstream webhook");
            }
        }
    });
}

/// Convenience: notify about a circuit breaker opening.
pub fn notify_circuit_open(upstream_url: &str) {
    notify(
        "circuit_open",
        upstream_url,
        &format!("Circuit breaker opened for {}", upstream_url),
    );
}

/// Convenience: notify about an upstream server error.
pub fn notify_upstream_error(upstream_url: &str, status: u16) {
    notify(
        "upstream_error",
        upstream_url,
        &format!("Upstream returned HTTP {} for {}", status, upstream_url),
    );
}

/// Convenience: notify about an upstream timeout.
pub fn notify_timeout(upstream_url: &str) {
    notify(
        "upstream_timeout",
        upstream_url,
        &format!("Upstream request timed out for {}", upstream_url),
    );
}
