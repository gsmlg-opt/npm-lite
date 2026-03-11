use axum::{
    extract::{Query, State},
    response::Html,
};
use sea_orm::{EntityTrait, PaginatorTrait, QueryOrder};
use serde::Deserialize;
use tracing::instrument;

use npm_entity::{packages, publish_events, users};

use crate::{
    error::WebResult,
    state::AppState,
    templates::{html_escape, layout, page_heading},
};

#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    pub page: Option<u64>,
}

#[instrument(skip(state))]
pub async fn activity_page(
    State(state): State<AppState>,
    Query(query): Query<ActivityQuery>,
) -> WebResult<Html<String>> {
    let db = &state.db;
    let page = query.page.unwrap_or(0);
    const PAGE_SIZE: u64 = 50;

    let paginator = publish_events::Entity::find()
        .order_by_desc(publish_events::Column::CreatedAt)
        .paginate(db, PAGE_SIZE);

    let total_pages = paginator.num_pages().await?;
    let events = paginator.fetch_page(page).await?;

    // Fetch all packages and users for display (small registry, eager load is fine)
    let all_packages = packages::Entity::find().all(db).await?;
    let all_users = users::Entity::find().all(db).await?;

    let pkg_map: std::collections::HashMap<uuid::Uuid, String> = all_packages
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect();

    let user_map: std::collections::HashMap<uuid::Uuid, String> = all_users
        .into_iter()
        .map(|u| (u.id, u.username))
        .collect();

    let rows: String = events
        .iter()
        .map(|evt| {
            let pkg_name = pkg_map
                .get(&evt.package_id)
                .map(|s| s.as_str())
                .unwrap_or("(unknown)");
            let actor = user_map
                .get(&evt.actor_id)
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
            let error = evt
                .error_message
                .as_deref()
                .map(|e| {
                    format!(
                        r#"<br /><span class="text-xs opacity-60 text-error">{}</span>"#,
                        html_escape(e),
                    )
                })
                .unwrap_or_default();
            let ts = evt.created_at.format("%Y-%m-%d %H:%M:%S UTC").to_string();
            format!(
                r#"<tr>
  <td class="text-sm opacity-60 font-mono whitespace-nowrap">{ts}</td>
  <td>
    <a href="/admin/packages/{pkg}" class="link link-primary font-mono">{pkg}</a>
  </td>
  <td>{action_badge}</td>
  <td class="font-mono text-sm">{actor}</td>
  <td>{status}{error}</td>
</tr>"#,
                pkg = html_escape(pkg_name),
                actor = html_escape(actor),
            )
        })
        .collect();

    let table = if rows.is_empty() {
        r#"<p class="opacity-60 mt-4">No activity recorded yet.</p>"#.to_string()
    } else {
        format!(
            r#"<div class="overflow-x-auto">
<table class="table table-zebra table-sm">
  <thead>
    <tr>
      <th>Time</th>
      <th>Package</th>
      <th>Action</th>
      <th>Actor</th>
      <th>Result</th>
    </tr>
  </thead>
  <tbody>{rows}</tbody>
</table>
</div>"#,
        )
    };

    // Pagination
    let pagination = if total_pages > 1 {
        let prev = if page > 0 {
            format!(
                r#"<a href="/admin/activity?page={p}" class="btn btn-sm">«</a>"#,
                p = page - 1,
            )
        } else {
            r#"<button class="btn btn-sm btn-disabled">«</button>"#.to_string()
        };
        let next = if page + 1 < total_pages {
            format!(
                r#"<a href="/admin/activity?page={p}" class="btn btn-sm">»</a>"#,
                p = page + 1,
            )
        } else {
            r#"<button class="btn btn-sm btn-disabled">»</button>"#.to_string()
        };
        format!(
            r#"<div class="flex gap-2 mt-4 items-center">
  {prev}
  <span class="text-sm opacity-60">Page {} of {}</span>
  {next}
</div>"#,
            page + 1,
            total_pages,
        )
    } else {
        String::new()
    };

    let content = format!(
        "{heading}{table}{pagination}",
        heading = page_heading("Publish Activity"),
    );

    Ok(Html(layout("Activity", &content)))
}
