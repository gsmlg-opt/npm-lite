//! Admin pages for upstream proxy configuration and cache management.

use axum::{
    extract::State,
    response::Html,
};
use tracing::instrument;

use crate::{
    error::WebResult,
    state::AppState,
    templates::{alert, html_escape, layout, page_heading},
};

/// `GET /admin/upstream` — Upstream settings and cache management page.
#[instrument(skip(state))]
pub async fn upstream_page(State(state): State<AppState>) -> WebResult<Html<String>> {
    let upstream = &state.upstream;
    let cache_count = if upstream.is_some() {
        npm_upstream::count_cached_packuments(&state.db)
            .await
            .unwrap_or(0)
    } else {
        0
    };

    let (upstream_status, upstream_url, cache_enabled, cache_ttl, scope_rules, local_scopes) =
        match upstream {
            Some(client) => {
                let cfg = client.config();
                (
                    "Enabled",
                    cfg.upstream_url
                        .as_deref()
                        .unwrap_or("(none)")
                        .to_string(),
                    cfg.cache_enabled,
                    cfg.cache_ttl.as_secs(),
                    cfg.scope_rules.clone(),
                    cfg.local_scopes.clone(),
                )
            }
            None => (
                "Disabled",
                "(not configured)".to_string(),
                false,
                300u64,
                std::collections::HashMap::new(),
                Vec::new(),
            ),
        };

    // Build scope rules table rows.
    let scope_rows = if scope_rules.is_empty() && local_scopes.is_empty() {
        r#"<tr><td colspan="2" class="text-center opacity-50">No scope rules configured</td></tr>"#
            .to_string()
    } else {
        let mut rows = String::new();
        for scope in &local_scopes {
            rows.push_str(&format!(
                r#"<tr><td class="font-mono">{scope}</td><td><span class="badge badge-error">local (never proxy)</span></td></tr>"#,
                scope = html_escape(scope),
            ));
        }
        for (scope, target) in &scope_rules {
            let badge = if target == "local" {
                r#"<span class="badge badge-error">local</span>"#.to_string()
            } else {
                format!(
                    r#"<span class="badge badge-info font-mono text-xs">{}</span>"#,
                    html_escape(target)
                )
            };
            rows.push_str(&format!(
                r#"<tr><td class="font-mono">{scope}</td><td>{badge}</td></tr>"#,
                scope = html_escape(scope),
                badge = badge,
            ));
        }
        rows
    };

    let content = format!(
        r#"{heading}

<div class="grid grid-cols-1 md:grid-cols-2 gap-6">

  <!-- Upstream Status -->
  <div class="card bg-base-200 shadow">
    <div class="card-body">
      <h2 class="card-title text-lg">Upstream Proxy</h2>
      <div class="overflow-x-auto">
        <table class="table">
          <tbody>
            <tr><th class="w-1/3">Status</th><td><span class="badge {status_badge}">{upstream_status}</span></td></tr>
            <tr><th>Upstream URL</th><td class="font-mono text-sm">{upstream_url}</td></tr>
            <tr><th>Caching</th><td><span class="badge {cache_badge}">{cache_status}</span></td></tr>
            <tr><th>Cache TTL</th><td>{cache_ttl}s</td></tr>
          </tbody>
        </table>
      </div>
      <div class="card-actions justify-end mt-2">
        <p class="text-xs opacity-50">Configure via <code>UPSTREAM_URL</code>, <code>UPSTREAM_CACHE_ENABLED</code>, or <code>upstream.toml</code></p>
      </div>
    </div>
  </div>

  <!-- Cache Stats -->
  <div class="card bg-base-200 shadow">
    <div class="card-body">
      <h2 class="card-title text-lg">Metadata Cache</h2>
      <div class="stat bg-base-300 rounded-box">
        <div class="stat-title">Cached Packages</div>
        <div class="stat-value text-primary">{cache_count}</div>
        <div class="stat-desc">packument metadata entries</div>
      </div>
      {purge_form}
    </div>
  </div>

  <!-- Scope Rules -->
  <div class="card bg-base-200 shadow md:col-span-2">
    <div class="card-body">
      <h2 class="card-title text-lg">Scope Routing Rules</h2>
      <p class="text-sm opacity-70 mb-2">
        Configure in <code>upstream.toml</code> under <code>[upstream.scopes]</code> and <code>[upstream.local_scopes]</code>.
      </p>
      <div class="overflow-x-auto">
        <table class="table">
          <thead>
            <tr><th>Scope</th><th>Target</th></tr>
          </thead>
          <tbody>
            {scope_rows}
          </tbody>
        </table>
      </div>
    </div>
  </div>

  <!-- Configuration Help -->
  <div class="card bg-base-200 shadow md:col-span-2">
    <div class="card-body">
      <h2 class="card-title text-lg">Configuration</h2>
      <p class="text-sm opacity-70 mb-2">Environment variables:</p>
      <div class="mockup-code text-sm">
        <pre><code>UPSTREAM_URL=https://registry.npmjs.org
UPSTREAM_CACHE_ENABLED=true
UPSTREAM_CACHE_TTL_SECS=300
UPSTREAM_TIMEOUT_SECS=30
UPSTREAM_CONFIG_PATH=/path/to/upstream.toml</code></pre>
      </div>
      <p class="text-sm opacity-70 mt-4 mb-2">Example <code>upstream.toml</code>:</p>
      <div class="mockup-code text-sm">
        <pre><code>[upstream]
url = "https://registry.npmjs.org"
cache_enabled = true
cache_ttl_secs = 300

[upstream.local_scopes]
scopes = ["@mycompany", "@internal"]

[upstream.scopes]
"@partner" = "https://partner-registry.example.com"</code></pre>
      </div>
    </div>
  </div>

</div>"#,
        heading = page_heading("Upstream Proxy"),
        upstream_status = html_escape(upstream_status),
        status_badge = if upstream_status == "Enabled" {
            "badge-success"
        } else {
            "badge-ghost"
        },
        upstream_url = html_escape(&upstream_url),
        cache_status = if cache_enabled { "Enabled" } else { "Disabled" },
        cache_badge = if cache_enabled {
            "badge-success"
        } else {
            "badge-ghost"
        },
        cache_ttl = cache_ttl,
        cache_count = cache_count,
        purge_form = if cache_enabled && cache_count > 0 {
            r#"<form method="POST" action="/admin/upstream/purge-cache" class="mt-4"
                    onsubmit="return confirm('Purge all cached upstream metadata?')">
                <button type="submit" class="btn btn-error btn-sm">Purge All Metadata Cache</button>
              </form>"#
        } else {
            ""
        },
        scope_rows = scope_rows,
    );

    Ok(Html(layout("Upstream Proxy", &content)))
}

/// `POST /admin/upstream/purge-cache` — Purge all cached upstream metadata.
#[instrument(skip(state))]
pub async fn purge_cache(State(state): State<AppState>) -> WebResult<Html<String>> {
    let deleted = npm_upstream::delete_all_cached_packuments(&state.db)
        .await
        .unwrap_or(0);

    // Also delete cached tarballs from S3.
    let tarball_deleted = match state.storage.list_objects(Some("upstream/")).await {
        Ok(objects) => {
            let mut count = 0u64;
            for obj in &objects {
                if state.storage.delete(&obj.key).await.is_ok() {
                    count += 1;
                }
            }
            count
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to list upstream cache objects for purge");
            0
        }
    };

    let msg = format!(
        "Purged {} metadata entries and {} cached tarballs.",
        deleted, tarball_deleted
    );

    let content = format!(
        "{}{}\n<p><a href=\"/admin/upstream\" class=\"btn btn-primary btn-sm mt-4\">Back to Upstream</a></p>",
        page_heading("Cache Purged"),
        alert("success", &msg),
    );

    Ok(Html(layout("Cache Purged", &content)))
}
