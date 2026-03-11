use axum::{extract::State, response::Html};
use tracing::instrument;

use crate::{
    error::WebResult,
    state::AppState,
    templates::{html_escape, layout, page_heading},
};

#[instrument(skip(state))]
pub async fn settings_page(State(state): State<AppState>) -> WebResult<Html<String>> {
    let registry_url = &state.config.registry_url;

    let content = format!(
        r#"{heading}

<div class="grid grid-cols-1 md:grid-cols-2 gap-6">

  <div class="card bg-base-200 shadow">
    <div class="card-body">
      <h2 class="card-title text-lg">Registry Configuration</h2>
      <div class="overflow-x-auto">
        <table class="table">
          <tbody>
            <tr><th class="w-1/3">Registry URL</th><td class="font-mono text-sm">{registry_url}</td></tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>

  <div class="card bg-base-200 shadow">
    <div class="card-body">
      <h2 class="card-title text-lg">Client Configuration</h2>
      <p class="text-sm opacity-70 mb-2">
        Add this to your <code class="font-mono">.npmrc</code> to use this registry:
      </p>
      <div class="mockup-code text-sm">
        <pre><code>registry={registry_url}
//{registry_host}/:_authToken=YOUR_TOKEN</code></pre>
      </div>
      <p class="text-sm opacity-70 mt-2">
        Or use <code class="font-mono">npm login</code>:
      </p>
      <div class="mockup-code text-sm">
        <pre><code>npm login --registry={registry_url}</code></pre>
      </div>
    </div>
  </div>

</div>"#,
        heading = page_heading("Settings"),
        registry_url = html_escape(registry_url),
        registry_host = html_escape(
            registry_url
                .strip_prefix("https://")
                .or_else(|| registry_url.strip_prefix("http://"))
                .unwrap_or(registry_url)
        ),
    );

    Ok(Html(layout("Settings", &content)))
}
