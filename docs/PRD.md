# PRD: Internal npm Registry for Team Usage (v2)

# 1. Overview

### Product Name

Working name: **Team npm Registry**

### Product Goal

Build an internal npm-compatible registry for team usage, implemented in **Rust**, with:

- **Axum** for backend HTTP/API
- **Leptos** for web UI (islands / partial hydration architecture)
- **Tailwind CSS + daisyUI** for UI styling
- **SeaORM** for database ORM
- **PostgreSQL** for metadata storage
- **S3** for tarball/blob storage

The product should support:

- private package hosting for team packages
- npm-compatible install and publish workflows (including `npm login`)
- token-based authentication and authorization
- package/version/tag management
- admin web UI with SSR shell and interactive islands
- optional upstream proxy/cache for public npm packages in later phases

This is an **internal team registry**, not a full replacement for npmjs.org.

# 2. Problem Statement

Teams often need a private package registry for:

- sharing internal packages
- controlling access to proprietary packages
- improving dependency governance
- avoiding reliance on public infrastructure for all installs
- centralizing package publish/install workflows

Existing solutions may be:

- too heavy
- too generic
- not aligned with a Rust-first stack
- difficult to customize for internal workflow and UI needs

The team wants a focused internal registry that is:

- easy to operate
- easy to extend
- Rust-native end to end
- compatible with standard npm tooling

# 3. Objectives

### Primary Objectives

- Provide a private npm-compatible registry for internal team packages
- Allow standard npm, pnpm, and yarn clients to install and publish packages
- Provide a web UI for package management and administration
- Store metadata in PostgreSQL and tarballs in S3
- Use a Rust-only application stack

### Secondary Objectives

- Support scoped packages (any valid npm scope allowed, no restriction)
- Provide dist-tag management such as latest, beta, next
- Provide team/user/token-based permission control
- Prepare for future upstream proxy support

### Non-Objectives

- Full npm account system parity
- Full npm search API parity
- Full audit/advisory mirror in v1
- External public package marketplace features
- Multi-tenant SaaS in v1
- Per-scope or per-package token granularity in v1

# 4. Users

### Primary Users

- internal developers
- release engineers
- platform/infrastructure team
- team administrators

### User Roles

- **Reader**: install private packages
- **Publisher**: publish/update package versions and tags
- **Admin**: manage users, teams, permissions, tokens, settings; soft-delete (unpublish) versions

# 5. User Stories

### Developer

- As a developer, I want to install private packages with standard npm tools.
- As a developer, I want package metadata to behave like a normal npm registry.
- As a developer, I want authentication to work with `.npmrc` and token-based auth.
- As a developer, I want to use `npm login` against the registry to obtain a token.

### Publisher

- As a publisher, I want to publish a scoped package with `npm publish`.
- As a publisher, I want published versions to be immutable (no overwrites).
- As a publisher, I want to update dist-tags like latest and beta.

### Admin

- As an admin, I want to view all hosted packages and versions.
- As an admin, I want to revoke tokens and manage permissions.
- As an admin, I want to inspect publish history and package ownership.
- As an admin, I want to manage package access rules by user/team.
- As an admin, I want to soft-delete (unpublish) a version while reserving the version slot.
- As an admin, I want to provision tokens via the admin UI.

# 6. Product Scope

## 6.1 In Scope for MVP

### Registry Features

- host private scoped packages (any valid npm scope)
- npm-compatible package metadata endpoints (packument reconstruction)
- npm-compatible tarball download endpoints (streamed through app)
- npm-compatible publish endpoint
- npm login / adduser endpoint (`/-/user/org.couchdb.user:*`)
- version storage with SRI integrity hashes (sha512)
- dist-tag support
- token-based auth (global scope: read / publish / admin)
- package-level/team-level access control
- admin-only soft-delete unpublish (version slot reserved, re-publish blocked)

### Admin UI Features

- SSR shell with interactive Leptos islands for forms, tables, search
- login/authenticated admin area
- package list
- package detail page
- version list (including soft-deleted versions marked as unpublished)
- dist-tag management
- token management (creation, revocation)
- team/user access management
- publish activity log

### Platform Features

- PostgreSQL metadata persistence
- S3 tarball storage (streamed through app, not presigned URLs)
- server-side validation (package name normalization, SRI hashes)
- audit/event logging for publish operations
- orphan blob cleanup (compensating delete + background GC)

## 6.2 Out of Scope for MVP

- public npm proxy/cache
- package deprecation workflow
- package analytics dashboards beyond basic activity logs
- replication/federation
- webhooks
- full provenance/signature support
- billing/quota systems
- per-scope or per-package token scoping

# 7. Functional Requirements

## 7.1 Registry Compatibility

The service must expose npm-compatible endpoints sufficient for standard package managers to:

- fetch package metadata (packument format)
- fetch version metadata
- download tarballs
- publish package versions
- resolve dist-tags
- authenticate via `npm login`

### Supported Flows

- `npm install`
- `pnpm install`
- `yarn add`
- `npm publish`
- `npm dist-tag add`
- `npm login` / `npm adduser`
- authenticated package fetch/install

### npm Login Support

The registry must implement the CouchDB-style user creation endpoint:

- `PUT /-/user/org.couchdb.user:{username}` — accepts username/password, returns a bearer token
- Token is stored hashed in PostgreSQL and returned to the client
- Client stores the token in `.npmrc` for subsequent requests

### Packument Reconstruction

Package metadata documents (packuments) are reconstructed on-the-fly from normalized PostgreSQL rows. The packument is the nested JSON blob that npm clients expect at `GET /{package}`, containing all versions, dist-tags, and per-version metadata inlined.

- No cached/denormalized packument JSON is stored
- PG queries join `packages`, `package_versions`, and `dist_tags` to build the response
- Caching layer may be added later if latency becomes an issue for packages with many versions

### Package Name Validation

Package names must follow npm normalization rules:

- lowercase only
- no leading dots or underscores
- no spaces or special characters beyond hyphens and dots
- max 214 characters
- scoped packages follow `@scope/name` format
- names are normalized and stored in canonical form
- uniqueness enforced at the normalized name level

### SRI Integrity Hashes

All published versions must include:

- `shasum` — SHA-1 hex digest of the tarball (for legacy npm compatibility)
- `integrity` — Subresource Integrity string using SHA-512 (e.g., `sha512-{base64}`)

These are computed server-side during publish and stored on `package_versions`. Clients use these to verify downloaded tarballs.

### Constraints

- any valid npm scope is allowed (no namespace restriction)
- published versions are immutable (no overwrite)
- soft-deleted versions reserve the version slot (re-publish blocked)
- tarball URLs resolve through the registry domain (streamed through app)

## 7.2 Package Management

The system must support:

- package creation on first publish
- storing multiple versions per package
- dist-tag assignment and update
- package ownership and access rules
- package visibility limited to internal users
- admin-only soft-delete of versions

### Rules

- package identity is based on package name and version
- version overwrite is not allowed
- soft-deleted versions remain in the database with a `deleted_at` timestamp
- soft-deleted version slots cannot be reclaimed (re-publish to same version is blocked)
- tags may be updated without republishing tarball
- package names must be normalized and unique

## 7.3 Authentication and Authorization

The system must support token-based authentication with `npm login` support.

### Token Types

- read token
- publish token
- admin token

### Token Scoping

MVP uses global scope only. A token's permissions apply to all packages that the token's role permits. Per-scope and per-package granularity may be added in a later phase.

### Authorization Rules

- read tokens can install/fetch all permitted packages
- publish tokens can publish to any package/scope
- admin tokens can manage registry state, settings, and unpublish versions

### Access Control

Support:

- team-level permissions
- package-level ACL
- scope-based publish rules (enforced via ACL, not token scoping)

### Token Storage

- tokens are stored as hashed values (e.g., SHA-256 or argon2 hash), never plaintext
- the raw token is returned only once at creation time
- token creation is available via admin UI and via `npm login` flow

## 7.4 Admin UI

The web UI is built with Leptos using an islands / partial hydration architecture:

- SSR shell renders page structure, navigation, and static content server-side
- Interactive islands provide client-side interactivity for forms, tables, search, and modals
- Minimal JS payload — only island code is sent to the client

### Dashboard

- package count
- version count
- recent publishes
- recent errors or failed operations

### Packages

- searchable package list (interactive island)
- package detail page
- version history (including soft-deleted versions, visually marked)
- dist-tags

### Access Management

- token creation/revocation (interactive island)
- team membership management
- package access rules

### Activity

- publish log
- unpublish log
- actor/time/result tracking

## 7.5 Storage

### PostgreSQL

Used for:

- packages
- package_versions (includes tarball metadata: s3_key, sha512, size)
- dist_tags
- users
- teams
- team_members
- tokens
- package_acl
- publish_events

### S3

Used for:

- tarball storage
- immutable package blobs

### Tarball Serving

Tarballs are streamed through the application:

- client requests tarball via registry URL
- app authenticates and checks read permission
- app streams tarball from S3 through the Axum response
- Caddy reverse proxies the connection; no direct S3 access is exposed

This keeps auth consistent at the app layer and avoids exposing S3 endpoint topology.

### Rules

- tarballs are never stored in PostgreSQL
- each `package_version` row references one S3 object via `s3_key`
- metadata commit should occur only after tarball is durably stored in S3
- orphan blobs (S3 objects without a corresponding `package_version`) are cleaned up via dual strategy (see §7.6)

## 7.6 Orphan Blob Cleanup

When the publish flow uploads a tarball to S3 but the subsequent PostgreSQL transaction fails, an orphan blob results. The system uses a dual cleanup strategy:

### Compensating Delete (Best-Effort)

- if the PG transaction fails after S3 upload, the app immediately attempts to delete the S3 object
- this is best-effort — if the delete also fails (network issue, app crash), the orphan remains

### Background GC Sweep (Safety Net)

- a periodic background task lists S3 objects and cross-references them against `package_versions.s3_key`
- S3 objects with no corresponding `package_version` row and older than a configurable threshold (default: 24 hours) are deleted
- the threshold prevents deleting blobs that are mid-publish (uploaded but not yet committed)
- GC runs are logged for audit purposes

# 8. Non-Functional Requirements

### Performance

- install metadata lookup should be low latency for internal team usage
- packument reconstruction from normalized rows should be efficient for packages with typical version counts (< 100 versions)
- tarball download should stream efficiently from S3 through the app layer
- system should support normal internal team concurrency without Redis

### Reliability

- published versions must not be partially committed (S3 upload before PG commit)
- system must tolerate orphaned blobs and clean them up automatically
- API errors should be explicit and traceable

### Security

- all endpoints served over HTTPS (terminated at Caddy)
- tokens stored hashed, not plaintext
- permission checks on publish/install/admin actions
- audit log for publish and unpublish operations
- `npm login` credentials validated server-side, token returned over HTTPS only

### Operability

- deployable as a single Rust binary
- configurable via environment variables
- suitable for container deployment
- compatible with Caddy reverse proxy
- background GC configurable (interval, orphan age threshold)

### Maintainability

- modular Rust codebase with explicit crate boundaries
- ORM-backed standard CRUD, with custom SQL for the publish transaction path and packument reconstruction
- Leptos islands keep frontend complexity contained to interactive components

# 9. Technical Architecture

## 9.1 Stack

### Backend

- Rust
- Axum
- SeaORM (with custom SQL for publish transaction and packument queries)
- PostgreSQL
- S3 SDK / abstraction layer

### Frontend

- Leptos (islands / partial hydration)
- Tailwind CSS
- daisyUI

### Infra

- Caddy (TLS termination, reverse proxy)
- PostgreSQL
- S3-compatible object storage

## 9.2 High-Level Architecture

```
Package Managers (npm/pnpm/yarn)
            |
            v
          Caddy (TLS termination)
            |
            v
   Rust App (Axum + Leptos Islands)
     - npm-compatible registry API
     - npm login endpoint
     - admin UI (SSR shell + islands)
     - auth/ACL
     - publish/install handling
     - packument reconstruction
     - activity logging
     - orphan GC background task
            |
            +--> PostgreSQL (metadata, tokens, ACL, events)
            |
            +--> S3 (tarball blobs)
```

## 9.3 Codebase Structure

```
npm-registry/
  Cargo.toml
  crates/
    core/        # domain logic, validation, shared types, package name normalization
    entity/      # SeaORM entities
    db/          # repositories, transactions, queries, packument reconstruction
    storage/     # S3 abstraction (upload, stream, delete, list for GC)
    registry/    # npm-compatible API handlers (metadata, tarball, publish, login)
    web/         # Leptos islands app (SSR shell + interactive components)
    app/         # binary, router composition, config, background GC task
```

# 10. Data Model

## 10.1 Core Entities

### Users
Internal registry users/admins.

### Teams
Logical groups for access control.

### Team Members
Maps users to teams.

### Tokens
Authentication credentials for read/publish/admin access. Stored hashed. Global scope only (no per-package/per-scope scoping in v1).

### Packages
Registry packages. Normalized name as unique key.

### Package Versions
Immutable published versions. Includes inline tarball metadata (`s3_key`, `sha512`, `size`, `shasum`, `integrity`). Supports soft-delete via `deleted_at` timestamp.

### Dist Tags
Maps tags like `latest` to versions.

### Package ACL
Permissions by team/user/package/scope.

### Publish Events
Audit trail for publish and unpublish attempts and results.

## 10.2 Table Set

- `users`
- `teams`
- `team_members`
- `tokens`
- `packages`
- `package_versions` (includes `s3_key`, `sha512`, `size`, `shasum`, `integrity`, `deleted_at`)
- `dist_tags`
- `package_acl`
- `publish_events`

Note: `tarball_objects` is **not** a separate table. Tarball metadata (S3 key, checksums, size) is inlined on `package_versions` since the relationship is 1:1.

## 10.3 Key Column Notes

### package_versions

| Column | Type | Notes |
|--------|------|-------|
| id | uuid | PK |
| package_id | uuid | FK to packages |
| version | text | semver string, unique per package |
| s3_key | text | S3 object key for tarball |
| sha512 | bytea | raw SHA-512 hash |
| shasum | text | SHA-1 hex digest (legacy npm compat) |
| integrity | text | SRI string (`sha512-{base64}`) |
| size | bigint | tarball size in bytes |
| metadata | jsonb | package.json fields (description, dependencies, etc.) |
| deleted_at | timestamptz | null = active, non-null = soft-deleted (unpublished) |
| created_at | timestamptz | publish timestamp |

### tokens

| Column | Type | Notes |
|--------|------|-------|
| id | uuid | PK |
| user_id | uuid | FK to users |
| token_hash | text | hashed token value (never plaintext) |
| role | enum | read / publish / admin |
| created_at | timestamptz | |
| revoked_at | timestamptz | null = active |

# 11. Key Workflows

## 11.1 Install Workflow

1. client requests package metadata (`GET /{package}`)
2. system authenticates bearer token
3. system checks package read permission via ACL
4. system reconstructs packument from normalized PG rows (packages + package_versions + dist_tags)
5. system returns packument JSON (excluding soft-deleted versions)
6. client requests tarball (`GET /{package}/-/{tarball}`)
7. system validates access
8. system streams tarball from S3 through Axum response

## 11.2 Publish Workflow

1. client authenticates publish token (bearer header)
2. system validates publish permission via ACL
3. system parses metadata and tarball from request body
4. system validates package name (normalization rules, length, allowed characters)
5. system computes SHA-1 (shasum) and SHA-512 (integrity) of tarball
6. system uploads tarball to S3, obtains `s3_key`
7. **BEGIN transaction:**
   - system checks version uniqueness (including soft-deleted versions — slot is reserved)
   - system creates package record if first publish
   - system creates `package_version` row with inline tarball metadata
   - system updates dist-tags
   - system records publish event
8. **COMMIT transaction**
9. on commit failure: attempt compensating S3 delete (best-effort)
10. system returns publish success response

## 11.3 npm Login Workflow

1. client sends `PUT /-/user/org.couchdb.user:{username}` with username and password
2. system validates credentials against stored user record
3. system generates a new token, hashes it, stores in `tokens` table with requested role (default: read)
4. system returns the raw token to the client (only time it is exposed)
5. client stores token in `.npmrc`

## 11.4 Unpublish Workflow (Admin Only)

1. admin authenticates with admin token or admin UI session
2. admin selects version to unpublish
3. system sets `deleted_at` on the `package_version` row (soft-delete)
4. system records unpublish event in `publish_events`
5. system does **not** delete the S3 blob (version slot is reserved, blob may be needed for audit)
6. subsequent installs for this version return 404
7. subsequent publishes to this version are rejected (slot reserved)

## 11.5 Admin Workflow

1. admin signs in (session-based auth via Leptos SSR)
2. admin views package list and details (SSR pages with interactive search island)
3. admin manages tokens or access (interactive islands for CRUD operations)
4. changes are recorded in audit/activity records

# 12. UI Requirements

## 12.1 UI Principles

- fast (SSR shell, minimal JS via islands)
- clean
- low-noise
- operational/admin-focused
- desktop-first, responsive enough for smaller screens
- no unnecessary JS-heavy component dependencies

## 12.2 Rendering Architecture

- Leptos islands / partial hydration
- SSR shell: page layout, navigation, static content rendered server-side
- Interactive islands: search bars, forms, data tables with sorting/filtering, modals
- Each island is independently hydrated — only island JS is shipped to client

## 12.3 Styling

- Tailwind CSS for layout/utilities
- daisyUI for component styling and theme system

## 12.4 Primary Screens

- Dashboard
- Packages
- Package Detail
- Versions & Dist-tags (soft-deleted versions visually distinguished)
- Tokens
- Teams & Access
- Publish Activity (includes unpublish events)
- Settings

# 13. API Requirements

## 13.1 Registry API

Must expose npm-compatible endpoints for:

- `GET /{package}` — packument (reconstructed from PG)
- `GET /{package}/{version}` — version metadata
- `GET /{package}/-/{tarball}` — tarball download (streamed from S3)
- `PUT /{package}` — publish
- `PUT /-/package/{package}/dist-tags/{tag}` — dist-tag set
- `DELETE /-/package/{package}/dist-tags/{tag}` — dist-tag remove
- `GET /-/package/{package}/dist-tags` — dist-tag list
- `PUT /-/user/org.couchdb.user:{username}` — npm login / adduser

All registry endpoints authenticate via Bearer token in Authorization header.

## 13.2 Admin/Internal API

May expose internal routes for:

- package search
- token management (create, list, revoke)
- access management (ACL CRUD)
- activity logs
- unpublish (admin only)
- settings
- orphan GC status

# 14. Success Metrics

### MVP Success Criteria

- team can publish private packages successfully
- team can install private packages using standard package managers
- team can authenticate via `npm login`
- package/version/tag state is correctly reflected in UI
- admin can manage tokens and access rules
- admin can unpublish versions (soft-delete)
- no manual DB/blob management needed for standard operations
- orphan blobs are cleaned up automatically

### Operational Metrics

- successful publish rate
- successful install rate
- average metadata response latency (packument reconstruction time)
- average tarball download latency
- number of registry errors
- number of auth failures
- orphan blob count / cleanup rate

# 15. Risks

### Compatibility Risk
npm protocol behavior can be subtle; endpoint shapes must match expected client behavior closely. The `npm login` flow and packument format are the highest-risk surfaces.

### Auth Risk
Incorrect token or ACL handling could expose internal packages or block valid installs.

### Storage Consistency Risk
S3 upload and DB commit must be coordinated carefully. Mitigated by compensating delete + background GC, but edge cases remain (e.g., GC deletes a blob that a concurrent publish is about to reference).

### Packument Reconstruction Performance Risk
On-the-fly packument reconstruction may become slow for packages with hundreds of versions. Mitigated by the assumption that internal packages rarely reach this scale; caching can be added later.

### ORM Overreach Risk
SeaORM is suitable for most CRUD, but the publish transaction and packument reconstruction will need custom SQL or explicit transaction management. This is acknowledged in the architecture.

### Scope Creep Risk
Trying to support full npmjs.org parity too early would slow MVP significantly.

# 16. Resolved Decisions

| Decision | Resolution | Rationale |
|----------|-----------|-----------|
| Tarball serving | Stream through app | Single origin, consistent auth at app layer |
| Unpublish policy | Admin-only soft-delete, version slot reserved | Pragmatic middle ground; immutability preserved for non-admins |
| npm login support | Yes, implement CouchDB-style endpoint | Developer ergonomics worth the protocol surface |
| Leptos rendering | Islands / partial hydration | Minimal JS, SSR for fast loads, islands for interactivity |
| Packument strategy | Reconstruct on-the-fly from normalized PG | Clean data model, avoids dual-write; caching can be added later |
| Token scoping | Global only in MVP | Simplifies auth layer; per-scope can be added in Phase 2 |
| Tarball metadata | Inline on package_versions | 1:1 relationship doesn't justify a separate table |
| Orphan cleanup | Compensating delete + background GC | Best-effort immediate cleanup with safety net |
| Package namespace | Any scope allowed | No restriction; flexible for team usage patterns |

# 17. Release Plan

## Phase 1 — MVP

- private scoped packages (any scope)
- package publish/install with SRI integrity
- npm login flow
- token auth (global scope)
- PostgreSQL metadata with inline tarball references
- S3 tarballs (streamed through app)
- packument reconstruction from normalized rows
- admin-only soft-delete unpublish
- orphan blob cleanup (compensating + GC)
- admin UI: Leptos islands for package/token/access management
- package name validation and normalization

## Phase 2

- per-scope token scoping
- better activity reporting
- package ownership improvements
- optional upstream public npm proxy/cache
- better policy controls
- packument response caching (if needed)

## Phase 3

- per-package token scoping
- advanced governance features
- deprecation workflow
- analytics
- CI-oriented publishing enhancements

# 18. Final Product Statement

This product is a **Rust-native internal npm registry** for team usage, focused on:

- standard npm compatibility (including `npm login` and SRI integrity)
- simple operations (single binary, stream-through architecture)
- strong ownership/access control (global tokens, team ACL, admin unpublish)
- PostgreSQL + S3 storage (normalized model, inline tarball metadata)
- Leptos-based admin UI (islands architecture for minimal client JS)
- maintainable full-stack Rust architecture (explicit crate boundaries, custom SQL where needed)

It should solve the team's private package distribution needs without attempting to become a full clone of npmjs.org.