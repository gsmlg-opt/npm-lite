# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

NPM Lite is a self-hosted npm registry written in Rust. It provides an npm-compatible API, S3-based tarball storage, PostgreSQL metadata, and an admin web UI.

## Build & Development Commands

```bash
# Start dependencies (PostgreSQL + MinIO)
docker compose up -d

# Copy and configure environment
cp .env.example .env

# Build the entire workspace
cargo build

# Build release binary
cargo build --release --bin npm-registry

# Run the server (applies migrations automatically on startup)
cargo run --bin npm-registry

# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p npm-db
cargo test -p npm-registry

# Check compilation without building
cargo check --workspace

# Lint
cargo clippy --workspace
```

## Architecture

### Workspace Crates (dependency flows downward)

```
npm-app          → Binary entrypoint, composes everything, spawns background GC
├── npm-registry → Axum router for npm wire protocol (token auth, publish, install)
├── npm-web      → Axum router for admin UI (session/cookie auth, server-rendered HTML)
├── npm-db       → Repository layer (SeaORM queries, transactional publish)
├── npm-storage  → S3 abstraction (upload/download/delete/GC)
├── npm-entity   → SeaORM entity models (generated, one file per table)
├── npm-core     → Pure functions: password hashing, token gen, validation, types
└── npm-migration→ SeaORM migration definitions
```

### Key Design Decisions

- **Two auth systems**: Registry API uses Bearer token auth (`AuthUser` extractor in `registry/src/auth.rs`). Admin UI uses cookie-based sessions (`AdminSession` middleware in `web/src/middleware.rs`).
- **Role hierarchy**: Admin > Publish > Read. Effective role = min(user.role, token.role).
- **Publish is transactional**: `db/src/publish.rs` wraps package creation, version insertion, dist-tag upsert, and event logging in a single DB transaction.
- **Soft-delete for versions**: `package_versions.deleted_at` is set on unpublish; versions are not physically removed. Dist-tags pointing to deleted versions must be cleaned up.
- **Background GC**: Compares S3 keys against DB references and deletes orphaned blobs. Configured via `GC_INTERVAL_SECS`.
- **Admin UI is server-rendered**: HTML built with string templates in `web/src/templates.rs`, no frontend framework.

### Routing

- `/admin/*` → `npm-web` router (admin UI)
- `/*` → `npm-registry` router (npm API)

Both routers share the same `AppState` (DB connection, S3 storage, registry config).

### Database

Single migration in `crates/migration/`. Tables: `users`, `tokens`, `teams`, `team_members`, `packages`, `package_versions`, `dist_tags`, `package_acl`, `publish_events`. Migrations run automatically on app startup.

### Environment Variables

Required: `DATABASE_URL`, `REGISTRY_URL`, `S3_BUCKET`
Optional: `S3_REGION`, `S3_ENDPOINT` (for MinIO), `BIND_ADDR`, `GC_INTERVAL_SECS`, `ADMIN_USERNAME`, `ADMIN_PASSWORD`, `RUST_LOG`

See `.env.example` for defaults.
