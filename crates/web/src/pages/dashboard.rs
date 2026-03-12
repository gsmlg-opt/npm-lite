use axum::{extract::State, response::Html};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};
use tracing::instrument;

use npm_entity::{package_versions, packages, publish_events};

use crate::{
    error::WebResult,
    state::AppState,
    templates::{layout, page_heading, stat_card},
};

#[instrument(skip(state))]
pub async fn dashboard_page(State(state): State<AppState>) -> WebResult<Html<String>> {
    let db = &state.db;

    // Run counts in parallel
    let (package_count, version_count, event_count) = tokio::try_join!(
        packages::Entity::find().count(db),
        package_versions::Entity::find().count(db),
        publish_events::Entity::find().count(db),
    )?;

    // Fetch 10 most recent publish events
    let recent_events = publish_events::Entity::find()
        .order_by_desc(publish_events::Column::CreatedAt)
        .limit(10)
        .all(db)
        .await?;

    // Build a package id -> name map for display
    let all_packages = packages::Entity::find().all(db).await?;
    let pkg_map: std::collections::HashMap<uuid::Uuid, String> =
        all_packages.into_iter().map(|p| (p.id, p.name)).collect();

    let stats = format!(
        r#"<div class="stats stats-horizontal shadow w-full mb-8 flex-wrap">
  {pkg}
  {ver}
  {evt}
</div>"#,
        pkg = stat_card(
            "Total Packages",
            &package_count.to_string(),
            "unique package names"
        ),
        ver = stat_card(
            "Total Versions",
            &version_count.to_string(),
            "published tarballs"
        ),
        evt = stat_card(
            "Publish Events",
            &event_count.to_string(),
            "all-time actions"
        ),
    );

    let rows: String = recent_events
        .iter()
        .map(|evt| {
            let pkg_name = pkg_map
                .get(&evt.package_id)
                .map(|s| s.as_str())
                .unwrap_or("(unknown)");
            let action_badge = if evt.action == "publish" {
                r#"<span class="badge badge-success badge-sm">publish</span>"#
            } else {
                r#"<span class="badge badge-error badge-sm">unpublish</span>"#
            };
            let status = if evt.success {
                r#"<span class="badge badge-outline badge-success badge-sm">ok</span>"#
            } else {
                r#"<span class="badge badge-outline badge-error badge-sm">fail</span>"#
            };
            let ts = evt.created_at.format("%Y-%m-%d %H:%M UTC").to_string();
            format!(
                r#"<tr>
  <td class="font-mono">{pkg_name}</td>
  <td>{action_badge}</td>
  <td>{status}</td>
  <td class="text-sm opacity-70">{ts}</td>
</tr>"#,
            )
        })
        .collect();

    let table = if rows.is_empty() {
        r#"<p class="text-sm opacity-60">No publish events yet.</p>"#.to_string()
    } else {
        format!(
            r#"<div class="overflow-x-auto">
<table class="table table-zebra">
  <thead>
    <tr>
      <th>Package</th>
      <th>Action</th>
      <th>Status</th>
      <th>Time</th>
    </tr>
  </thead>
  <tbody>{rows}</tbody>
</table>
</div>"#,
        )
    };

    // Fetch recent failed events.
    let failed_events = publish_events::Entity::find()
        .filter(publish_events::Column::Success.eq(false))
        .order_by_desc(publish_events::Column::CreatedAt)
        .limit(5)
        .all(db)
        .await?;

    let error_section = if failed_events.is_empty() {
        String::new()
    } else {
        let error_rows: String = failed_events
            .iter()
            .map(|evt| {
                let pkg_name = pkg_map
                    .get(&evt.package_id)
                    .map(|s| s.as_str())
                    .unwrap_or("(unknown)");
                let err_msg = evt.error_message.as_deref().unwrap_or("(no details)");
                let ts = evt.created_at.format("%Y-%m-%d %H:%M UTC").to_string();
                format!(
                    r#"<tr>
  <td class="font-mono">{pkg_name}</td>
  <td>{action}</td>
  <td class="text-sm text-error">{err_msg}</td>
  <td class="text-sm opacity-70">{ts}</td>
</tr>"#,
                    action = crate::templates::html_escape(&evt.action),
                    err_msg = crate::templates::html_escape(err_msg),
                )
            })
            .collect();

        format!(
            r#"<div class="card bg-base-200 shadow mb-6 border border-error/30">
  <div class="card-body">
    <h2 class="card-title text-lg text-error mb-4">Recent Errors</h2>
    <div class="overflow-x-auto">
      <table class="table table-zebra">
        <thead><tr><th>Package</th><th>Action</th><th>Error</th><th>Time</th></tr></thead>
        <tbody>{error_rows}</tbody>
      </table>
    </div>
  </div>
</div>"#,
        )
    };

    let content = format!(
        r#"{heading}
{stats}
{error_section}
<div class="card bg-base-200 shadow">
  <div class="card-body">
    <h2 class="card-title text-lg mb-4">Recent Activity</h2>
    {table}
  </div>
</div>"#,
        heading = page_heading("Dashboard"),
    );

    Ok(Html(layout("Dashboard", &content)))
}
