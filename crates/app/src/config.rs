use std::env;

/// Application configuration, loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection URL (required).
    /// Example: `postgres://user:pass@localhost/npm_registry`
    pub database_url: String,

    /// S3 bucket name for tarball storage (required).
    pub s3_bucket: String,

    /// AWS region for the S3 bucket (optional; defaults to `us-east-1`).
    pub s3_region: String,

    /// Custom S3 endpoint URL for S3-compatible stores such as MinIO (optional).
    pub s3_endpoint: Option<String>,

    /// Public base URL of this registry (required).
    /// Example: `https://registry.example.com`
    pub registry_url: String,

    /// Socket address the HTTP server will bind to.
    /// Defaults to `0.0.0.0:3000`.
    pub bind_addr: String,

    /// How often (in seconds) the background GC task runs.
    /// Set to `0` to disable GC entirely.
    pub gc_interval_secs: u64,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Panics with a descriptive message for any required variable that is missing.
    pub fn from_env() -> Self {
        Self {
            database_url: required("DATABASE_URL"),
            s3_bucket: required("S3_BUCKET"),
            s3_region: env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            s3_endpoint: env::var("S3_ENDPOINT").ok().filter(|s| !s.is_empty()),
            registry_url: required("REGISTRY_URL"),
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
            gc_interval_secs: env::var("GC_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600),
        }
    }
}

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("Required environment variable `{name}` is not set"))
}
