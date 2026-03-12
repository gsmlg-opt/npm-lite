# NPM Lite

A self-hosted, lightweight npm registry built in Rust.

## Features

- Full npm-compatible API (publish, install, search, dist-tags)
- Scoped and unscoped package support
- S3-compatible tarball storage (AWS S3 or MinIO)
- PostgreSQL metadata with SeaORM
- Token-based API authentication with role hierarchy (Read / Publish / Admin)
- Fine-grained access control (per-package and per-scope ACLs for users and teams)
- Admin web UI for managing packages, users, teams, tokens, and access
- Background garbage collection for orphaned S3 blobs
- Soft-delete unpublish with audit logging

## Quick Start

```bash
# Start PostgreSQL and MinIO
docker compose up -d

# Configure environment
cp .env.example .env

# Build and run (migrations run automatically)
cargo run --bin npm-registry
```

The registry is available at `http://localhost:3000` and the admin UI at `http://localhost:3000/admin`.

Default admin credentials: `admin` / `admin` (set via `ADMIN_USERNAME` / `ADMIN_PASSWORD`).

## Using with npm

```bash
# Point npm at your registry
npm set registry http://localhost:3000

# Login (creates a token)
npm login

# Publish a package
npm publish

# Install a package
npm install <package-name>
```

## Configuration

| Variable | Required | Default | Description |
|---|---|---|---|
| `DATABASE_URL` | Yes | | PostgreSQL connection string |
| `REGISTRY_URL` | Yes | | Public base URL of the registry |
| `S3_BUCKET` | Yes | | S3 bucket for tarballs |
| `S3_REGION` | No | `us-east-1` | AWS region |
| `S3_ENDPOINT` | No | | Custom S3 endpoint (for MinIO) |
| `BIND_ADDR` | No | `0.0.0.0:3000` | Listen address |
| `GC_INTERVAL_SECS` | No | `3600` | Orphan GC interval (0 to disable) |
| `ADMIN_USERNAME` | No | `admin` | Seed admin username |
| `ADMIN_PASSWORD` | No | `admin` | Seed admin password |
| `RUST_LOG` | No | | Tracing filter directive |

## Docker

```bash
docker build -t npm-lite .
docker run -p 3000:3000 --env-file .env npm-lite
```

## License

MIT
