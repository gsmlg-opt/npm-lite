use axum::{
    extract::{Path, Query, State},
    response::Html,
};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use serde::Deserialize;
use tracing::instrument;

use npm_entity::{dist_tags, package_versions, packages};

use crate::{
    error::{WebError, WebResult},
    state::AppState,
    templates::{html_escape, layout, page_heading},
};

#[derive(Debug, Deserialize)]
pub struct PackageListQuery {
    pub q: Option<String>,
    pub page: Option<u64>,
}

#[instrument(skip(state))]
pub async fn package_list_page(
    State(state): State<AppState>,
    Query(query): Query<PackageListQuery>,
) -> WebResult<Html<String>> {
    let db = &state.db;
    let search = query.q.as_deref().unwrap_or("").trim().to_string();
    let page = query.page.unwrap_or(0);
    const PAGE_SIZE: u64 = 20;

    let mut finder = packages::Entity::find();
    if !search.is_empty() {
        finder = finder.filter(
            packages::Column::Name.contains(&search),
        );
    }
    let paginator = finder
        .order_by_asc(packages::Column::Name)
        .paginate(db, PAGE_SIZE);

    let total_pages = paginator.num_pages().await?;
    let pkgs = paginator.fetch_page(page).await?;

    // Search form
    let search_form = format!(
        r#"<form method="get" action="/admin/packages" class="flex gap-2 mb-6">
  <input type="text" name="q" value="{val}" placeholder="Search packages…"
    class="input input-bordered w-full max-w-sm" />
  <button type="submit" class="btn btn-primary">Search</button>
  {clear}
</form>"#,
        val = html_escape(&search),
        clear = if search.is_empty() {
            String::new()
        } else {
            r#"<a href="/admin/packages" class="btn btn-ghost">Clear</a>"#.to_string()
        },
    );

    let rows: String = pkgs
        .iter()
        .map(|pkg| {
            let scope = pkg
                .scope
                .as_deref()
                .map(|s| format!(r#"<span class="badge badge-outline badge-sm">{}</span>"#, html_escape(s)))
                .unwrap_or_default();
            let desc = pkg
                .description
                .as_deref()
                .unwrap_or("—")
                .to_string();
            let ts = pkg.updated_at.format("%Y-%m-%d").to_string();
            format!(
                r#"<tr>
  <td>
    <a href="/admin/packages/{name}" class="link link-primary font-mono">{name}</a>
    {scope}
  </td>
  <td class="text-sm opacity-80">{desc}</td>
  <td class="text-sm opacity-60">{ts}</td>
</tr>"#,
                name = html_escape(&pkg.name),
                scope = scope,
                desc = html_escape(&desc),
            )
        })
        .collect();

    let table = if rows.is_empty() {
        r#"<p class="opacity-60 mt-4">No packages found.</p>"#.to_string()
    } else {
        format!(
            r#"<div class="overflow-x-auto">
<table class="table table-zebra">
  <thead><tr><th>Name</th><th>Description</th><th>Updated</th></tr></thead>
  <tbody>{rows}</tbody>
</table>
</div>"#,
        )
    };

    // Pagination
    let pagination = if total_pages > 1 {
        let prev = if page > 0 {
            format!(
                r#"<a href="/admin/packages?q={q}&page={p}" class="btn btn-sm">«</a>"#,
                q = html_escape(&search),
                p = page - 1,
            )
        } else {
            r#"<button class="btn btn-sm btn-disabled">«</button>"#.to_string()
        };
        let next = if page + 1 < total_pages {
            format!(
                r#"<a href="/admin/packages?q={q}&page={p}" class="btn btn-sm">»</a>"#,
                q = html_escape(&search),
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
        "{heading}{search_form}{table}{pagination}",
        heading = page_heading("Packages"),
    );

    Ok(Html(layout("Packages", &content)))
}

#[instrument(skip(state))]
pub async fn package_detail_page(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> WebResult<Html<String>> {
    let db = &state.db;

    let pkg = packages::Entity::find()
        .filter(packages::Column::Name.eq(&name))
        .one(db)
        .await?
        .ok_or(WebError::NotFound)?;

    let versions = package_versions::Entity::find()
        .filter(package_versions::Column::PackageId.eq(pkg.id))
        .filter(package_versions::Column::DeletedAt.is_null())
        .order_by_desc(package_versions::Column::CreatedAt)
        .all(db)
        .await?;

    let tags = dist_tags::Entity::find()
        .filter(dist_tags::Column::PackageId.eq(pkg.id))
        .all(db)
        .await?;

    // Build a tag map: version_id -> list of tags
    let mut tag_map: std::collections::HashMap<uuid::Uuid, Vec<String>> =
        std::collections::HashMap::new();
    for tag in &tags {
        tag_map
            .entry(tag.version_id)
            .or_default()
            .push(tag.tag.clone());
    }

    let desc = pkg.description.as_deref().unwrap_or("No description.");
    let scope = pkg
        .scope
        .as_deref()
        .map(|s| {
            format!(
                r#" <span class="badge badge-outline badge-sm ml-2">{}</span>"#,
                html_escape(s),
            )
        })
        .unwrap_or_default();

    let ver_rows: String = versions
        .iter()
        .map(|v| {
            let size_kb = v.size / 1024;
            let ts = v.created_at.format("%Y-%m-%d %H:%M UTC").to_string();
            let tag_badges: String = tag_map
                .get(&v.id)
                .map(|tags| {
                    tags.iter()
                        .map(|t| {
                            format!(
                                r#"<span class="badge badge-accent badge-sm mr-1">{}</span>"#,
                                html_escape(t),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            format!(
                r#"<tr>
  <td class="font-mono">{ver}</td>
  <td>{tags}</td>
  <td class="text-sm opacity-70">{size} KB</td>
  <td class="text-sm opacity-60">{ts}</td>
</tr>"#,
                ver = html_escape(&v.version),
                tags = tag_badges,
                size = size_kb,
            )
        })
        .collect();

    let ver_table = if ver_rows.is_empty() {
        r#"<p class="opacity-60 mt-4">No published versions.</p>"#.to_string()
    } else {
        format!(
            r#"<div class="overflow-x-auto mt-4">
<table class="table table-zebra">
  <thead><tr><th>Version</th><th>Tags</th><th>Size</th><th>Published</th></tr></thead>
  <tbody>{ver_rows}</tbody>
</table>
</div>"#,
        )
    };

    let content = format!(
        r#"<div class="mb-2 text-sm breadcrumbs">
  <ul><li><a href="/admin/packages">Packages</a></li><li>{name}</li></ul>
</div>
<h1 class="text-3xl font-bold mb-1 font-mono">{name}{scope}</h1>
<p class="text-base opacity-70 mb-6">{desc}</p>
<div class="card bg-base-200 shadow">
  <div class="card-body">
    <h2 class="card-title">Versions ({count})</h2>
    {ver_table}
  </div>
</div>"#,
        name = html_escape(&pkg.name),
        scope = scope,
        desc = html_escape(desc),
        count = versions.len(),
    );

    Ok(Html(layout(&pkg.name, &content)))
}
