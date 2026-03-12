//! HTTP client for upstream registry requests.

use bytes::Bytes;
use futures::Stream;
use reqwest::Client;
use std::pin::Pin;
use tracing::{debug, warn};

use crate::config::UpstreamConfig;
use crate::error::UpstreamError;

/// HTTP client wrapper for talking to upstream npm registries.
#[derive(Clone)]
pub struct UpstreamClient {
    client: Client,
    config: UpstreamConfig,
}

impl UpstreamClient {
    /// Create a new upstream client from the given configuration.
    pub fn new(config: UpstreamConfig) -> Result<Self, UpstreamError> {
        let client = Client::builder()
            .timeout(config.timeout)
            .pool_max_idle_per_host(10)
            .user_agent("npm-lite-upstream/0.1")
            .build()
            .map_err(UpstreamError::Request)?;

        Ok(Self { client, config })
    }

    /// Returns a reference to the upstream configuration.
    pub fn config(&self) -> &UpstreamConfig {
        &self.config
    }

    /// Returns the configured global upstream URL, or `Err(NoUpstream)` if none.
    fn upstream_url(&self) -> Result<&str, UpstreamError> {
        self.config
            .upstream_url
            .as_deref()
            .ok_or(UpstreamError::NoUpstream)
    }

    /// Fetch a packument (package metadata JSON) from the default upstream.
    pub async fn fetch_packument(
        &self,
        package_name: &str,
    ) -> Result<serde_json::Value, UpstreamError> {
        let base = self.upstream_url()?;
        self.fetch_packument_from(package_name, base).await
    }

    /// Fetch a packument from a specific upstream URL.
    pub async fn fetch_packument_from(
        &self,
        package_name: &str,
        upstream_url: &str,
    ) -> Result<serde_json::Value, UpstreamError> {
        let url = format!(
            "{}/{}",
            upstream_url.trim_end_matches('/'),
            package_name
        );

        debug!(url = %url, "fetching packument from upstream");

        let resp = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    UpstreamError::Timeout(url.clone())
                } else {
                    UpstreamError::Request(e)
                }
            })?;

        let status = resp.status().as_u16();
        match status {
            200 => {}
            404 => return Err(UpstreamError::NotFound(package_name.to_string())),
            s if s >= 500 => {
                return Err(UpstreamError::UpstreamServerError {
                    status: s,
                    url,
                });
            }
            _ => {
                warn!(status, url = %url, "unexpected upstream status");
                return Err(UpstreamError::UpstreamServerError {
                    status,
                    url,
                });
            }
        }

        let body = resp.text().await.map_err(UpstreamError::Request)?;
        let packument: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| {
                UpstreamError::InvalidResponse(format!(
                    "failed to parse packument for '{}': {}",
                    package_name, e
                ))
            })?;

        Ok(packument)
    }

    /// Stream a tarball from the upstream registry.
    ///
    /// Returns a byte stream suitable for forwarding to the client, plus
    /// the content length if provided by the upstream.
    pub async fn stream_tarball(
        &self,
        tarball_url: &str,
    ) -> Result<
        (
            Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
            Option<u64>,
        ),
        UpstreamError,
    > {
        debug!(url = %tarball_url, "streaming tarball from upstream");

        let resp = self
            .client
            .get(tarball_url)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    UpstreamError::Timeout(tarball_url.to_string())
                } else {
                    UpstreamError::Request(e)
                }
            })?;

        let status = resp.status().as_u16();
        match status {
            200 => {}
            404 => return Err(UpstreamError::NotFound(tarball_url.to_string())),
            s if s >= 500 => {
                return Err(UpstreamError::UpstreamServerError {
                    status: s,
                    url: tarball_url.to_string(),
                });
            }
            _ => {
                return Err(UpstreamError::UpstreamServerError {
                    status,
                    url: tarball_url.to_string(),
                });
            }
        }

        let content_length = resp.content_length();
        let stream = resp.bytes_stream();

        Ok((Box::pin(stream), content_length))
    }

    /// Download a tarball fully into memory (for caching to S3).
    pub async fn download_tarball(
        &self,
        tarball_url: &str,
    ) -> Result<Bytes, UpstreamError> {
        debug!(url = %tarball_url, "downloading tarball from upstream for caching");

        let resp = self
            .client
            .get(tarball_url)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    UpstreamError::Timeout(tarball_url.to_string())
                } else {
                    UpstreamError::Request(e)
                }
            })?;

        let status = resp.status().as_u16();
        match status {
            200 => {}
            404 => return Err(UpstreamError::NotFound(tarball_url.to_string())),
            s if s >= 500 => {
                return Err(UpstreamError::UpstreamServerError {
                    status: s,
                    url: tarball_url.to_string(),
                });
            }
            _ => {
                return Err(UpstreamError::UpstreamServerError {
                    status,
                    url: tarball_url.to_string(),
                });
            }
        }

        resp.bytes().await.map_err(UpstreamError::Request)
    }
}
