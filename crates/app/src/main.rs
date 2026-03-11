mod config;
mod gc;

use std::sync::Arc;

use axum::Router;
use sea_orm::Database;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use npm_registry::{registry_router, AppState, RegistryConfig};
use npm_storage::S3Storage;
use npm_web::web_router;

use config::Config;
use gc::spawn_gc_task;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Load .env file (non-fatal if missing) ──────────────────────────────
    dotenvy::dotenv().ok();

    // ── Initialise tracing ─────────────────────────────────────────────────
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // ── Load configuration ─────────────────────────────────────────────────
    let cfg = Config::from_env();

    info!(
        bind_addr = %cfg.bind_addr,
        registry_url = %cfg.registry_url,
        s3_bucket = %cfg.s3_bucket,
        "starting npm-lite registry"
    );

    // ── Connect to PostgreSQL ──────────────────────────────────────────────
    let db = Database::connect(&cfg.database_url).await?;
    info!("connected to database");

    // ── Run database migrations ────────────────────────────────────────────
    {
        use npm_migration::MigratorTrait;
        npm_migration::Migrator::up(&db, None).await?;
    }
    info!("database migrations applied");

    // ── Seed admin user on first boot ─────────────────────────────────────
    {
        let admin_username =
            std::env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string());
        let admin_password =
            std::env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string());

        match npm_db::UserRepo::find_by_username(&db, &admin_username).await? {
            Some(_) => {
                info!(username = %admin_username, "admin user already exists");
            }
            None => {
                let password_hash = npm_core::auth::hash_password(&admin_password)
                    .map_err(|e| anyhow::anyhow!("failed to hash admin password: {}", e))?;
                npm_db::UserRepo::create(
                    &db,
                    &admin_username,
                    password_hash,
                    format!("{}@localhost", admin_username),
                    "admin",
                )
                .await?;
                info!(username = %admin_username, "admin user created");
            }
        }
    }

    // ── Create S3 storage ──────────────────────────────────────────────────
    let storage = build_storage(&cfg).await?;
    info!(bucket = %cfg.s3_bucket, "S3 storage initialised");

    // ── Build shared application state ────────────────────────────────────
    let state = AppState {
        db,
        storage: Arc::new(storage),
        config: RegistryConfig {
            registry_url: cfg.registry_url.clone(),
        },
    };

    // ── Spawn background GC task ───────────────────────────────────────────
    spawn_gc_task(state.clone(), cfg.gc_interval_secs);

    // ── Compose the application router ────────────────────────────────────
    //
    // Registry API routes at "/" and admin UI routes at "/admin".
    let app: Router = Router::new()
        .nest("/admin", web_router().with_state(state.clone()))
        .merge(registry_router().with_state(state))
        .layer(TraceLayer::new_for_http());

    // ── Start the HTTP server ──────────────────────────────────────────────
    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr).await?;
    info!(addr = %cfg.bind_addr, "listening");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Build an [`S3Storage`] from the loaded configuration.
///
/// If `S3_ENDPOINT` is set the AWS SDK is configured to talk to a
/// S3-compatible endpoint (e.g. MinIO) at that URL.
async fn build_storage(cfg: &Config) -> anyhow::Result<S3Storage> {
    use aws_config::BehaviorVersion;
    use aws_sdk_s3::{config::Region, Client};

    // SAFETY: called before any multi-threaded S3 access.
    unsafe { std::env::set_var("AWS_DEFAULT_REGION", &cfg.s3_region) };

    let mut sdk_builder = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(cfg.s3_region.clone()));

    if let Some(endpoint) = &cfg.s3_endpoint {
        sdk_builder = sdk_builder.endpoint_url(endpoint);
    }

    let aws_cfg = sdk_builder.load().await;

    // For S3-compatible stores, force path-style addressing.
    let s3_config = aws_sdk_s3::config::Builder::from(&aws_cfg)
        .force_path_style(cfg.s3_endpoint.is_some())
        .build();

    let client = Client::from_conf(s3_config);

    Ok(S3Storage::new(client, cfg.s3_bucket.clone()))
}
