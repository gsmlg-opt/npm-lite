use std::sync::Arc;

/// Configuration values for the registry.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Base URL of this registry, e.g. `"https://registry.example.com"`.
    pub registry_url: String,
}

/// Shared application state threaded through every Axum handler via
/// `axum::extract::State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    pub db: sea_orm::DatabaseConnection,
    pub storage: Arc<npm_storage::S3Storage>,
    pub config: RegistryConfig,
}
