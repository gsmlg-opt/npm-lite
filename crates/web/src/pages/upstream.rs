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
    let stats = npm_upstream::cache_stats();

    let (
        upstream_status,
        upstream_url,
        cache_enabled,
        cache_ttl,
        scope_rules,
        local_scopes,
        pattern_rules,
    ) = match upstream {
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
                cfg.pattern_rules.clone(),
            )
        }
        None => (
            "Disabled",
            "(not configured)".to_string(),
            false,
            300u64,
            std::collections::HashMap::new(),
            Vec::new(),
            Vec::new(),
        ),
    };

    // Load DB-stored rules.
    let db_rules = npm_upstream::list_rules(&state.db).await.unwrap_or_default();

    // Build scope rules table rows (from config files).
    let scope_rows = if scope_rules.is_empty() && local_scopes.is_empty() {
        r#"<tr><td colspan="2" class="text-center opacity-50">No file-based scope rules</td></tr>"#
            .to_string()
    } else {
        let mut rows = String::new();
        for scope in &local_scopes {
            rows.push_str(&format!(
                r#"<tr><td class="font-mono">{scope}</td><td><span class="badge badge-error">local (never proxy)</span></td><td class="text-xs opacity-50">config</td></tr>"#,
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
                r#"<tr><td class="font-mono">{scope}</td><td>{badge}</td><td class="text-xs opacity-50">config</td></tr>"#,
                scope = html_escape(scope),
                badge = badge,
            ));
        }
        rows
    };

    // Build pattern rules table rows (from config files).
    let pattern_rows = if pattern_rules.is_empty() {
        r#"<tr><td colspan="4" class="text-center opacity-50">No file-based pattern rules</td></tr>"#
            .to_string()
    } else {
        let mut rows = String::new();
        for (i, rule) in pattern_rules.iter().enumerate() {
            let badge = if rule.target == "local" {
                r#"<span class="badge badge-error">local</span>"#.to_string()
            } else {
                format!(
                    r#"<span class="badge badge-info font-mono text-xs">{}</span>"#,
                    html_escape(&rule.target)
                )
            };
            rows.push_str(&format!(
                r#"<tr><td>{order}</td><td class="font-mono text-sm">{pattern}</td><td>{badge}</td><td class="text-xs opacity-50">config</td></tr>"#,
                order = i + 1,
                pattern = html_escape(&rule.pattern),
                badge = badge,
            ));
        }
        rows
    };

    // Build DB rules table rows.
    let db_rules_rows = if db_rules.is_empty() {
        r#"<tr><td colspan="7" class="text-center opacity-50">No database rules configured. Use the form below to add rules.</td></tr>"#.to_string()
    } else {
        let mut rows = String::new();
        for rule in &db_rules {
            let type_badge = match rule.rule_type.as_str() {
                "global" => r#"<span class="badge badge-primary">global</span>"#,
                "scope" => r#"<span class="badge badge-secondary">scope</span>"#,
                "pattern" => r#"<span class="badge badge-accent">pattern</span>"#,
                _ => r#"<span class="badge">unknown</span>"#,
            };
            let target_badge = if rule.upstream_url == "local" {
                r#"<span class="badge badge-error">local</span>"#.to_string()
            } else {
                format!(
                    r#"<span class="badge badge-info font-mono text-xs">{}</span>"#,
                    html_escape(&rule.upstream_url)
                )
            };
            let enabled_badge = if rule.enabled {
                r#"<span class="badge badge-success badge-sm">on</span>"#
            } else {
                r#"<span class="badge badge-ghost badge-sm">off</span>"#
            };
            rows.push_str(&format!(
                r#"<tr>
                  <td>{type_badge}</td>
                  <td class="font-mono text-sm">{match_val}</td>
                  <td>{target_badge}</td>
                  <td>{priority}</td>
                  <td>{enabled_badge}</td>
                  <td class="font-mono text-xs">{auth}</td>
                  <td>
                    <button class="btn btn-error btn-xs" onclick="deleteRule('{id}')">Delete</button>
                  </td>
                </tr>"#,
                type_badge = type_badge,
                match_val = html_escape(&rule.match_value),
                target_badge = target_badge,
                priority = rule.priority,
                enabled_badge = enabled_badge,
                auth = if rule.auth_token_ref.is_some() { "***" } else { "-" },
                id = rule.id,
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
      <div class="stats stats-vertical bg-base-300 rounded-box w-full">
        <div class="stat">
          <div class="stat-title">Cached Packages</div>
          <div class="stat-value text-primary">{cache_count}</div>
        </div>
        <div class="stat">
          <div class="stat-title">Hits / Misses / Stale</div>
          <div class="stat-value text-sm">{cache_hits} / {cache_misses} / {cache_stale}</div>
          <div class="stat-desc">since server start</div>
        </div>
      </div>
      {purge_form}
    </div>
  </div>

  <!-- File-based Scope Rules -->
  <div class="card bg-base-200 shadow md:col-span-2">
    <div class="card-body">
      <h2 class="card-title text-lg">Scope Routing Rules (from config)</h2>
      <div class="overflow-x-auto">
        <table class="table">
          <thead>
            <tr><th>Scope</th><th>Target</th><th>Source</th></tr>
          </thead>
          <tbody>
            {scope_rows}
          </tbody>
        </table>
      </div>
    </div>
  </div>

  <!-- File-based Pattern Rules -->
  <div class="card bg-base-200 shadow md:col-span-2">
    <div class="card-body">
      <h2 class="card-title text-lg">Pattern Routing Rules (from config)</h2>
      <div class="overflow-x-auto">
        <table class="table">
          <thead>
            <tr><th>#</th><th>Pattern (regex)</th><th>Target</th><th>Source</th></tr>
          </thead>
          <tbody>
            {pattern_rows}
          </tbody>
        </table>
      </div>
    </div>
  </div>

  <!-- DB-managed Rules -->
  <div class="card bg-base-200 shadow md:col-span-2">
    <div class="card-body">
      <h2 class="card-title text-lg">Database-managed Rules</h2>
      <p class="text-sm opacity-70 mb-2">
        Rules stored in the database and manageable via this UI. DB rules are merged with file-based rules.
      </p>
      <div class="overflow-x-auto">
        <table class="table" id="db-rules-table">
          <thead>
            <tr><th>Type</th><th>Match</th><th>Target</th><th>Priority</th><th>Enabled</th><th>Auth</th><th>Actions</th></tr>
          </thead>
          <tbody>
            {db_rules_rows}
          </tbody>
        </table>
      </div>

      <!-- Add Rule Form -->
      <div class="divider">Add New Rule</div>
      <form id="add-rule-form" class="grid grid-cols-1 md:grid-cols-6 gap-3 items-end">
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">Type</span></label>
          <select name="rule_type" class="select select-bordered select-sm" required>
            <option value="scope">scope</option>
            <option value="pattern">pattern</option>
            <option value="global">global</option>
          </select>
        </div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">Match Value</span></label>
          <input type="text" name="match_value" class="input input-bordered input-sm" placeholder="@scope or ^pattern.*" required />
        </div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">Upstream URL</span></label>
          <input type="text" name="upstream_url" class="input input-bordered input-sm" placeholder="https://... or local" required />
        </div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">Priority</span></label>
          <input type="number" name="priority" class="input input-bordered input-sm" value="0" />
        </div>
        <div class="form-control">
          <label class="label"><span class="label-text text-xs">Auth Token Ref</span></label>
          <input type="text" name="auth_token_ref" class="input input-bordered input-sm" placeholder="env:VAR_NAME" />
        </div>
        <button type="submit" class="btn btn-primary btn-sm">Add Rule</button>
      </form>
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
"@partner" = "https://partner-registry.example.com"

[[upstream.patterns]]
pattern = "^internal-.*"
target = "local"

[[upstream.patterns]]
pattern = "^legacy-.*"
target = "https://legacy-registry.example.com"</code></pre>
      </div>
    </div>
  </div>

</div>

<script>
async function deleteRule(id) {{
  if (!confirm('Delete this rule?')) return;
  try {{
    const resp = await fetch('/admin/api/upstream/rules/' + id, {{ method: 'DELETE' }});
    if (resp.ok) {{
      location.reload();
    }} else {{
      const data = await resp.json();
      alert('Error: ' + (data.error || 'Unknown error'));
    }}
  }} catch (e) {{
    alert('Network error: ' + e.message);
  }}
}}

document.getElementById('add-rule-form').addEventListener('submit', async function(e) {{
  e.preventDefault();
  const form = e.target;
  const body = {{
    rule_type: form.rule_type.value,
    match_value: form.match_value.value,
    upstream_url: form.upstream_url.value,
    priority: parseInt(form.priority.value) || 0,
    enabled: true,
    auth_token_ref: form.auth_token_ref.value || null,
  }};
  try {{
    const resp = await fetch('/admin/api/upstream/rules', {{
      method: 'POST',
      headers: {{ 'Content-Type': 'application/json' }},
      body: JSON.stringify(body),
    }});
    if (resp.ok || resp.status === 201) {{
      location.reload();
    }} else {{
      const data = await resp.json();
      alert('Error: ' + (data.error || 'Unknown error'));
    }}
  }} catch (e) {{
    alert('Network error: ' + e.message);
  }}
}});
</script>"#,
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
        cache_hits = stats.hits,
        cache_misses = stats.misses,
        cache_stale = stats.stale_hits,
        purge_form = if cache_enabled && cache_count > 0 {
            r#"<form method="POST" action="/admin/upstream/purge-cache" class="mt-4"
                    onsubmit="return confirm('Purge all cached upstream metadata?')">
                <button type="submit" class="btn btn-error btn-sm">Purge All Metadata Cache</button>
              </form>"#
        } else {
            ""
        },
        scope_rows = scope_rows,
        pattern_rows = pattern_rows,
        db_rules_rows = db_rules_rows,
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
