use axum::{
    extract::{Path, Query, State},
    response::{Html, Redirect},
    Extension, Form,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use npm_entity::{dist_tags, package_versions, packages, publish_events};

use crate::{
    error::{WebError, WebResult},
    middleware::AdminSession,
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

    // Fetch ALL versions (including soft-deleted) so they are shown in the UI.
    let versions = package_versions::Entity::find()
        .filter(package_versions::Column::PackageId.eq(pkg.id))
        .order_by_desc(package_versions::Column::CreatedAt)
        .all(db)
        .await?;

    let active_count = versions.iter().filter(|v| v.deleted_at.is_none()).count();

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

    // Build version_id -> version string map for dist-tag display
    let version_map: std::collections::HashMap<uuid::Uuid, String> = versions
        .iter()
        .map(|v| (v.id, v.version.clone()))
        .collect();

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

    let escaped_name = html_escape(&pkg.name);

    let ver_rows: String = versions
        .iter()
        .map(|v| {
            let size_kb = v.size / 1024;
            let ts = v.created_at.format("%Y-%m-%d %H:%M UTC").to_string();
            let is_deleted = v.deleted_at.is_some();
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
            let deleted_badge = if is_deleted {
                r#" <span class="badge badge-error badge-sm">unpublished</span>"#
            } else {
                ""
            };
            let row_class = if is_deleted { r#" class="opacity-50""# } else { "" };
            let ver_style = if is_deleted { " line-through" } else { "" };
            let unpublish_btn = if !is_deleted {
                format!(
                    r#"<form method="post" action="/admin/packages/{name}/versions/{ver}/unpublish" class="inline">
  <button type="submit" class="btn btn-xs btn-error"
    onclick="return confirm('Unpublish version {ver}? This cannot be undone.')">Unpublish</button>
</form>"#,
                    name = escaped_name,
                    ver = html_escape(&v.version),
                )
            } else {
                String::new()
            };
            format!(
                r#"<tr{row_class}>
  <td class="font-mono{ver_style}">{ver}{deleted_badge}</td>
  <td>{tags}</td>
  <td class="text-sm opacity-70">{size} KB</td>
  <td class="text-sm opacity-60">{ts}</td>
  <td>{unpublish_btn}</td>
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
  <thead><tr><th>Version</th><th>Tags</th><th>Size</th><th>Published</th><th></th></tr></thead>
  <tbody>{ver_rows}</tbody>
</table>
</div>"#,
        )
    };

    // Dist-tag table rows
    let tag_rows: String = tags
        .iter()
        .map(|t| {
            let ver_str = version_map
                .get(&t.version_id)
                .map(|s| html_escape(s))
                .unwrap_or_else(|| "(unknown)".to_string());
            let delete_btn = if t.tag != "latest" {
                format!(
                    r#"<form method="post" action="/admin/packages/{name}/dist-tags/{tag}/delete" class="inline">
  <button type="submit" class="btn btn-xs btn-error"
    onclick="return confirm('Remove dist-tag {tag}?')">Remove</button>
</form>"#,
                    name = escaped_name,
                    tag = html_escape(&t.tag),
                )
            } else {
                String::new()
            };
            format!(
                r#"<tr>
  <td class="font-mono">{tag}</td>
  <td class="font-mono">{ver}</td>
  <td>{delete_btn}</td>
</tr>"#,
                tag = html_escape(&t.tag),
                ver = ver_str,
            )
        })
        .collect();

    let tag_table = if tag_rows.is_empty() {
        r#"<p class="opacity-60 mt-4">No dist-tags.</p>"#.to_string()
    } else {
        format!(
            r#"<div class="overflow-x-auto mt-4">
<table class="table table-zebra">
  <thead><tr><th>Tag</th><th>Version</th><th></th></tr></thead>
  <tbody>{tag_rows}</tbody>
</table>
</div>"#,
        )
    };

    // Active version options for the dist-tag form
    let version_options: String = versions
        .iter()
        .filter(|v| v.deleted_at.is_none())
        .map(|v| {
            format!(
                r#"<option value="{id}">{ver}</option>"#,
                id = v.id,
                ver = html_escape(&v.version),
            )
        })
        .collect();

    let dist_tag_form = format!(
        r#"<form method="post" action="/admin/packages/{name}/dist-tags" class="flex gap-3 items-end mt-4">
  <label class="form-control">
    <div class="label"><span class="label-text">Tag name</span></div>
    <input type="text" name="tag" placeholder="e.g. latest, beta, next" required class="input input-bordered input-sm" />
  </label>
  <label class="form-control">
    <div class="label"><span class="label-text">Version</span></div>
    <select name="version_id" required class="select select-bordered select-sm">
      {version_options}
    </select>
  </label>
  <button type="submit" class="btn btn-primary btn-sm">Set Tag</button>
</form>"#,
        name = escaped_name,
    );

    let content = format!(
        r#"<div class="mb-2 text-sm breadcrumbs">
  <ul><li><a href="/admin/packages">Packages</a></li><li>{name}</li></ul>
</div>
<h1 class="text-3xl font-bold mb-1 font-mono">{name}{scope}</h1>
<p class="text-base opacity-70 mb-6">{desc}</p>
<div class="card bg-base-200 shadow">
  <div class="card-body">
    <h2 class="card-title">Versions ({active_count} active, {total_count} total)</h2>
    {ver_table}
  </div>
</div>
<div class="card bg-base-200 shadow mt-6">
  <div class="card-body">
    <h2 class="card-title">Dist-tags</h2>
    {tag_table}
    {dist_tag_form}
  </div>
</div>"#,
        name = escaped_name,
        scope = scope,
        desc = html_escape(desc),
        active_count = active_count,
        total_count = versions.len(),
    );

    Ok(Html(layout(&pkg.name, &content)))
}

// --- Unpublish version handler ---

#[derive(Debug, Deserialize)]
pub struct VersionUnpublishPath {
    pub name: String,
    pub version: String,
}

#[instrument(skip(state, session))]
pub async fn version_unpublish(
    State(state): State<AppState>,
    Extension(session): Extension<AdminSession>,
    Path(path): Path<VersionUnpublishPath>,
) -> WebResult<Redirect> {
    use chrono::Utc;

    let db = &state.db;

    let pkg = packages::Entity::find()
        .filter(packages::Column::Name.eq(&path.name))
        .one(db)
        .await?
        .ok_or(WebError::NotFound)?;

    let version = package_versions::Entity::find()
        .filter(package_versions::Column::PackageId.eq(pkg.id))
        .filter(package_versions::Column::Version.eq(&path.version))
        .filter(package_versions::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or(WebError::NotFound)?;

    let version_id = version.id;

    // Set deleted_at on the version
    let mut active: package_versions::ActiveModel = version.into();
    active.deleted_at = Set(Some(Utc::now().into()));
    active.update(db).await?;

    // Remove dist-tags pointing to this version so they don't become orphaned.
    dist_tags::Entity::delete_many()
        .filter(dist_tags::Column::PackageId.eq(pkg.id))
        .filter(dist_tags::Column::VersionId.eq(version_id))
        .exec(db)
        .await?;

    // Record unpublish event with actual admin user ID.
    let event = publish_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        package_id: Set(pkg.id),
        version_id: Set(Some(version_id)),
        action: Set("unpublish".to_string()),
        actor_id: Set(session.user_id),
        success: Set(true),
        error_message: Set(None),
        created_at: Set(Utc::now().into()),
    };
    event.insert(db).await?;

    Ok(Redirect::to(&format!("/admin/packages/{}", pkg.name)))
}

// --- Dist-tag set handler ---

#[derive(Debug, Deserialize)]
pub struct DistTagSetForm {
    pub tag: String,
    pub version_id: Uuid,
}

#[instrument(skip(state, form))]
pub async fn dist_tag_set(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Form(form): Form<DistTagSetForm>,
) -> WebResult<Redirect> {
    use chrono::Utc;

    let db = &state.db;

    let pkg = packages::Entity::find()
        .filter(packages::Column::Name.eq(&name))
        .one(db)
        .await?
        .ok_or(WebError::NotFound)?;

    let tag_name = form.tag.trim().to_string();
    if tag_name.is_empty() {
        return Ok(Redirect::to(&format!("/admin/packages/{}", pkg.name)));
    }

    // Check if the tag already exists for this package
    let existing = dist_tags::Entity::find()
        .filter(dist_tags::Column::PackageId.eq(pkg.id))
        .filter(dist_tags::Column::Tag.eq(&tag_name))
        .one(db)
        .await?;

    if let Some(existing_tag) = existing {
        // Update the existing tag to point to the new version
        let mut active: dist_tags::ActiveModel = existing_tag.into();
        active.version_id = Set(form.version_id);
        active.updated_at = Set(Utc::now().into());
        active.update(db).await?;
    } else {
        // Create a new dist-tag
        let now = Utc::now().into();
        let model = dist_tags::ActiveModel {
            id: Set(Uuid::new_v4()),
            package_id: Set(pkg.id),
            tag: Set(tag_name),
            version_id: Set(form.version_id),
            created_at: Set(now),
            updated_at: Set(now),
        };
        model.insert(db).await?;
    }

    Ok(Redirect::to(&format!("/admin/packages/{}", pkg.name)))
}

// --- Dist-tag delete handler ---

#[derive(Debug, Deserialize)]
pub struct DistTagDeletePath {
    pub name: String,
    pub tag: String,
}

#[instrument(skip(state))]
pub async fn dist_tag_delete(
    State(state): State<AppState>,
    Path(path): Path<DistTagDeletePath>,
) -> WebResult<Redirect> {
    let db = &state.db;

    let pkg = packages::Entity::find()
        .filter(packages::Column::Name.eq(&path.name))
        .one(db)
        .await?
        .ok_or(WebError::NotFound)?;

    // Don't allow deleting the "latest" tag
    if path.tag == "latest" {
        return Ok(Redirect::to(&format!("/admin/packages/{}", pkg.name)));
    }

    let tag = dist_tags::Entity::find()
        .filter(dist_tags::Column::PackageId.eq(pkg.id))
        .filter(dist_tags::Column::Tag.eq(&path.tag))
        .one(db)
        .await?
        .ok_or(WebError::NotFound)?;

    dist_tags::Entity::delete_by_id(tag.id).exec(db).await?;

    Ok(Redirect::to(&format!("/admin/packages/{}", pkg.name)))
}
