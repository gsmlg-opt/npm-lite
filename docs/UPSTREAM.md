# PRD: Upstream Registry Proxy & Cache

# 1. Overview

### Feature Name

**Upstream Registry Proxy**

### Feature Goal

Add upstream registry support to npm-lite so the registry can proxy and optionally cache packages from remote npm registries. This allows teams to use npm-lite as the single registry endpoint for both private and public packages, eliminating the need for `.npmrc` multi-registry configuration.

Three levels of upstream routing are supported:

1. **Global upstream** — a default fallback registry (typically `https://registry.npmjs.org`)
2. **Per-scope upstream** — route specific `@scope` prefixes to designated registries
3. **Per-pattern upstream** — route package names matching regex patterns to designated registries

### Motivation

Without upstream support, developers must configure multiple registries in `.npmrc` (e.g., the private registry for `@company/*` and npmjs.org for everything else). This creates friction, leaks scope names to public registries during resolution, and makes air-gapped or compliance-controlled setups impossible.

# 2. User Stories

### Developer

- As a developer, I want to run `npm install` against one registry URL and get both private and public packages.
- As a developer, I want scoped packages like `@company/*` resolved from my private registry and all other packages proxied from npmjs.org transparently.
- As a developer, I want cached upstream packages to install faster on repeat installs.

### Admin

- As an admin, I want to configure a global upstream registry as a fallback.
- As an admin, I want to map specific scopes (e.g., `@babel`) to specific upstream registries.
- As an admin, I want to define regex-based routing rules for non-scoped package names.
- As an admin, I want to enable or disable upstream caching independently of proxying.
- As an admin, I want to see which packages are proxied vs. locally published in the admin UI.
- As an admin, I want to manage upstream configuration through the admin UI or environment/config file.

### Security / Compliance

- As a security engineer, I want the registry to never forward requests for private scopes to public upstreams.
- As a compliance officer, I want the option to block upstream access entirely (air-gapped mode) while still serving cached packages.

# 3. Upstream Routing Model

## 3.1 Rule Evaluation Order

When a package is requested and not found locally, the system evaluates upstream rules in the following priority order:

1. **Per-scope rules** — if the package is scoped (`@scope/name`), check for an exact scope match
2. **Per-pattern rules** — check the full package name against regex patterns (evaluated in defined order, first match wins)
3. **Global upstream** — if no scope or pattern rule matched, use the global upstream (if configured)
4. **No upstream** — if no rule matched and no global upstream is set, return 404

## 3.2 Rule Types

### Global Upstream

A single default upstream registry URL.

```
UPSTREAM_URL=https://registry.npmjs.org
```

### Per-Scope Upstream

Maps a scope prefix to an upstream registry URL.

```toml
[upstream.scopes]
"@mycompany" = "local"           # never proxy, always local-only
"@partner" = "https://partner-registry.example.com"
"@babel" = "https://registry.npmjs.org"
```

The special value `"local"` means "do not proxy this scope; return 404 if not found locally." This is critical for preventing private scope names from leaking to public registries.

### Per-Pattern Upstream

Maps a regex pattern to an upstream registry URL. Patterns are evaluated in order; first match wins.

```toml
[[upstream.patterns]]
pattern = "^internal-.*"
target = "local"

[[upstream.patterns]]
pattern = "^legacy-.*"
target = "https://legacy-registry.example.com"
```

## 3.3 Configuration

Upstream configuration can be provided via:

1. **Environment variables** — for simple global upstream setup
2. **Configuration file** (`upstream.toml`) — for scope and pattern rules
3. **Database** — for admin UI-managed rules (Phase 2)

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `UPSTREAM_URL` | Global upstream registry URL | _(none — no upstream)_ |
| `UPSTREAM_CACHE_ENABLED` | Cache proxied metadata and tarballs locally | `false` |
| `UPSTREAM_CACHE_TTL_SECS` | How long cached metadata is considered fresh | `300` (5 minutes) |
| `UPSTREAM_TIMEOUT_SECS` | HTTP timeout for upstream requests | `30` |
| `UPSTREAM_CONFIG_PATH` | Path to `upstream.toml` for scope/pattern rules | _(none)_ |

### Configuration File (`upstream.toml`)

```toml
[upstream]
url = "https://registry.npmjs.org"
cache_enabled = true
cache_ttl_secs = 300
timeout_secs = 30

# Scopes that should never be proxied upstream
[upstream.local_scopes]
scopes = ["@mycompany", "@internal"]

# Per-scope routing to specific upstreams
[upstream.scopes]
"@partner" = "https://partner-registry.example.com"

# Pattern-based routing (evaluated in order, first match wins)
[[upstream.patterns]]
pattern = "^internal-.*"
target = "local"

[[upstream.patterns]]
pattern = "^legacy-.*"
target = "https://legacy-registry.example.com"
```

# 4. Functional Requirements

## 4.1 Proxy Behavior

### Packument Proxy

When a `GET /{package}` request finds no local package:

1. Evaluate upstream routing rules to determine the target upstream
2. If target is `"local"`, return 404
3. Forward the request to the upstream registry
4. If upstream returns a packument, rewrite tarball URLs to point to this registry (so tarballs are also proxied)
5. Return the rewritten packument to the client
6. If caching is enabled, store the packument metadata in the database

### Tarball Proxy

When a `GET /{package}/-/{tarball}` request has no local tarball:

1. Determine the upstream for this package (same routing logic)
2. Stream the tarball from the upstream through this registry to the client
3. If caching is enabled, simultaneously upload the tarball to S3 (tee the stream)
4. Verify integrity (shasum/integrity) of the streamed tarball against the packument metadata

### Merged Packument (Local + Upstream)

If a package exists both locally and upstream (e.g., a fork or override scenario), the local version always takes precedence:

- Local versions are included as-is
- Upstream versions are included only if no local version with the same version string exists
- Dist-tags from local always override upstream dist-tags
- This behavior can be disabled per-scope via a `"local"` rule (never merge, always local-only)

## 4.2 Caching

### Metadata Cache

- Cached packuments are stored in a new `upstream_packages` table (or a flag on existing `packages`)
- Cache entries have a `fetched_at` timestamp and are refreshed when older than `cache_ttl_secs`
- Stale cache entries are still served if the upstream is unreachable (stale-while-error)

### Tarball Cache

- Cached tarballs are stored in S3 with a distinct key prefix (e.g., `upstream/{package}/{version}.tgz`)
- Cached tarballs reference the upstream `integrity` value for verification
- Cached tarball records are stored in the database with an `upstream_url` field to distinguish them from locally published packages

### Cache Invalidation

- Metadata cache is time-based (TTL)
- Tarball cache is permanent (immutable content-addressed blobs)
- Admin can manually purge cached packages via admin UI or API
- Background GC should also clean up orphaned upstream cache blobs

## 4.3 Authentication to Upstreams

Some upstream registries require authentication.

```toml
[upstream.scopes]
"@partner" = { url = "https://partner-registry.example.com", token = "env:PARTNER_REGISTRY_TOKEN" }
```

- Tokens can be specified inline or referenced from environment variables via `env:VAR_NAME`
- Bearer token auth is supported (same as npm's `_authToken`)
- Credentials are never logged or exposed in API responses

## 4.4 Error Handling

| Scenario | Behavior |
|----------|----------|
| Upstream timeout | Return 504 Gateway Timeout (or serve stale cache if available) |
| Upstream 404 | Return 404 to client, do not cache |
| Upstream 5xx | Return 502 Bad Gateway (or serve stale cache if available) |
| Upstream returns invalid packument | Return 502, log error, do not cache |
| Upstream unreachable + cache hit | Serve stale cache, log warning |
| Upstream unreachable + no cache | Return 504 |

## 4.5 Security

- **Scope isolation**: Scopes listed in `local_scopes` are never forwarded upstream, preventing scope name leakage
- **No credential forwarding**: Client auth tokens are never forwarded to upstreams; each upstream uses its own configured credentials
- **Integrity verification**: Cached tarballs are verified against the upstream-provided integrity hash
- **Air-gapped mode**: If no upstream URL is configured, the registry operates in fully local mode (current behavior preserved)

# 5. Data Model Changes

### New Table: `upstream_configs`

| Column | Type | Notes |
|--------|------|-------|
| id | uuid | PK |
| rule_type | text | `global`, `scope`, `pattern` |
| match_value | text | scope name, regex pattern, or `*` for global |
| upstream_url | text | target URL or `local` |
| auth_token_ref | text | nullable; `env:VAR_NAME` or encrypted token |
| priority | integer | evaluation order (lower = higher priority) |
| enabled | boolean | soft-disable without deleting |
| created_at | timestamptz | |
| updated_at | timestamptz | |

### New Table: `upstream_cache`

| Column | Type | Notes |
|--------|------|-------|
| id | uuid | PK |
| package_name | text | full package name (unique) |
| upstream_url | text | which upstream this was fetched from |
| packument_json | jsonb | cached packument metadata |
| fetched_at | timestamptz | when the cache entry was last refreshed |
| created_at | timestamptz | |

### Modified Table: `package_versions`

| New Column | Type | Notes |
|------------|------|-------|
| source | text | `local` or `upstream`; default `local` |
| upstream_url | text | nullable; origin upstream URL for cached versions |

# 6. Architecture

## 6.1 New Crate: `npm-upstream`

```
npm-upstream    → Upstream proxy logic, HTTP client, caching, routing
├── client.rs   → HTTP client for upstream registries (reqwest-based)
├── config.rs   → Upstream configuration parsing (env + TOML)
├── router.rs   → Rule evaluation: scope → pattern → global → none
├── proxy.rs    → Packument rewriting, tarball streaming with tee
├── cache.rs    → Cache read/write/invalidation logic
└── error.rs    → Upstream-specific error types
```

### Dependency Position

```
npm-app
├── npm-registry → uses npm-upstream for proxy fallback
├── npm-upstream → new crate
│   ├── npm-db
│   ├── npm-storage
│   └── npm-core
├── npm-web      → admin UI for upstream config
├── npm-db
├── npm-storage
├── npm-entity
├── npm-core
└── npm-migration
```

## 6.2 Request Flow (with upstream)

```
Client → GET /@lodash/merge
  │
  ├─ 1. Check local DB → not found
  │
  ├─ 2. Evaluate upstream rules
  │     scope "@lodash" → no scope rule
  │     pattern match → no match
  │     global upstream → https://registry.npmjs.org
  │
  ├─ 3. Check upstream cache → miss (or stale)
  │
  ├─ 4. Fetch from upstream
  │     GET https://registry.npmjs.org/@lodash/merge
  │
  ├─ 5. Rewrite tarball URLs
  │     registry.npmjs.org/... → this-registry/...
  │
  ├─ 6. Cache packument (if enabled)
  │
  └─ 7. Return rewritten packument to client
```

```
Client → GET /@lodash/merge/-/merge-4.6.2.tgz
  │
  ├─ 1. Check local S3 → not found
  │
  ├─ 2. Check upstream cache (S3 prefix upstream/) → miss
  │
  ├─ 3. Stream from upstream (tee to S3 if caching)
  │
  ├─ 4. Verify integrity
  │
  └─ 5. Return tarball stream to client
```

# 7. Admin UI

### Upstream Settings Page

- View and edit global upstream URL
- Manage scope rules (add/edit/remove)
- Manage pattern rules (add/edit/remove/reorder)
- Toggle caching on/off
- Configure cache TTL

### Package List Enhancement

- Show package source indicator: `local` | `cached` | `proxied`
- Filter by source type

### Cache Management

- View cached package count and total size
- Purge individual cached packages
- Purge all cached packages
- View cache hit/miss statistics

# 8. API

### Admin Upstream API

- `GET /admin/api/upstream/config` — get current upstream configuration
- `PUT /admin/api/upstream/config` — update upstream configuration
- `GET /admin/api/upstream/rules` — list all routing rules
- `POST /admin/api/upstream/rules` — create a routing rule
- `PUT /admin/api/upstream/rules/{id}` — update a routing rule
- `DELETE /admin/api/upstream/rules/{id}` — delete a routing rule
- `DELETE /admin/api/upstream/cache` — purge all cached packages
- `DELETE /admin/api/upstream/cache/{package}` — purge a specific cached package

# 9. Non-Functional Requirements

### Performance

- Upstream proxy requests should add minimal latency over direct upstream access
- Tarball streaming should not buffer the entire tarball in memory; use streaming/chunked transfer
- Cache hits should be served with the same latency as locally published packages
- Upstream HTTP client should use connection pooling

### Reliability

- Upstream failures must not break local package resolution
- Stale cache should be preferred over hard failure when upstream is unavailable
- Upstream timeout should be configurable and have a reasonable default (30s)

### Security

- Private scopes are never leaked to upstream registries
- Upstream credentials are stored securely and never exposed in API responses
- Cached content integrity is verified before serving

### Operability

- Upstream mode is opt-in; default behavior is unchanged (no upstream, fully local)
- All upstream operations are logged for debugging and audit
- Metrics: upstream request count, cache hit rate, upstream error rate, upstream latency

# 10. Implementation Phases

## Phase 1 — Global Upstream Proxy (No Cache)

- Add `UPSTREAM_URL` environment variable
- Implement HTTP client for upstream requests (reqwest)
- Proxy packument requests: local miss → fetch from upstream → rewrite tarball URLs → return
- Proxy tarball requests: local miss → stream from upstream → return
- No caching, no persistence of upstream data
- No scope/pattern rules (global fallback only)

## Phase 2 — Caching + Scope Rules

- Add `upstream.toml` configuration file support
- Implement per-scope routing rules
- Implement `local_scopes` (never-proxy list)
- Add metadata cache (new DB table)
- Add tarball cache (S3 with `upstream/` prefix)
- Cache TTL and stale-while-error behavior
- Admin UI: upstream settings page, cache management

## Phase 3 — Pattern Rules + Admin UI Management

- Implement regex pattern-based routing rules
- Add `upstream_configs` database table
- Admin UI: CRUD for scope and pattern rules
- Admin UI: package source indicators
- Cache hit/miss statistics

## Phase 4 — Advanced Features

- Upstream authentication (bearer tokens per upstream)
- Upstream health checks and circuit breaker
- Cache warming (pre-fetch popular packages)
- Cache size limits and eviction policies
- Webhook notifications for upstream failures

# 11. Risks

### Compatibility Risk
Upstream registries may return packument formats with fields or structures that our rewriting logic doesn't handle correctly. Mitigated by thorough testing against npmjs.org responses.

### Performance Risk
Proxying adds latency for every uncached upstream request. Mitigated by caching and connection pooling.

### Security Risk
Misconfigured scope rules could leak private scope names to public registries. Mitigated by `local_scopes` deny-list and clear documentation.

### Storage Risk
Caching all upstream packages could consume significant S3 storage. Mitigated by making caching opt-in and adding cache eviction in Phase 4.

### Upstream Availability Risk
Registry availability becomes coupled to upstream availability for uncached packages. Mitigated by stale-while-error caching and clear error responses.

# 12. Resolved Decisions

| Decision | Resolution | Rationale |
|----------|-----------|-----------|
| Local vs. upstream precedence | Local always wins | Prevents upstream from overriding private packages |
| Scope leak prevention | Explicit `local_scopes` deny-list | Simple, auditable, fail-safe |
| Cache storage | Same S3 bucket with `upstream/` prefix | Reuses existing infra, clear separation |
| Tarball proxy strategy | Stream-through with optional tee to S3 | No memory buffering, consistent with existing tarball serving |
| Configuration format | Env vars + TOML file | Env vars for simple cases, TOML for complex routing |
| Default behavior | No upstream (current behavior preserved) | Opt-in, zero breaking changes |
| Rule evaluation order | Scope → Pattern → Global | Most specific first, predictable resolution |
