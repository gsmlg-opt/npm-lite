use axum::{
    Form,
    extract::{Path, State},
    response::{Html, Redirect},
};
use sea_orm::{EntityTrait, QueryOrder};
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use npm_db::AclRepo;
use npm_entity::{package_acl, packages, teams, users};

use crate::{
    error::WebResult,
    state::AppState,
    templates::{html_escape, layout, page_heading},
};

#[instrument(skip(state))]
pub async fn access_page(State(state): State<AppState>) -> WebResult<Html<String>> {
    let db = &state.db;

    // Load all ACL entries
    let all_acls = package_acl::Entity::find()
        .order_by_desc(package_acl::Column::CreatedAt)
        .all(db)
        .await?;

    // Load all packages, users, teams for lookups and dropdowns
    let all_packages = packages::Entity::find()
        .order_by_asc(packages::Column::Name)
        .all(db)
        .await?;

    let all_users = users::Entity::find()
        .order_by_asc(users::Column::Username)
        .all(db)
        .await?;

    let all_teams = teams::Entity::find()
        .order_by_asc(teams::Column::Name)
        .all(db)
        .await?;

    // Build table rows
    let rows: String = all_acls
        .iter()
        .map(|acl| {
            let pkg_name = match acl.package_id {
                Some(pid) => all_packages
                    .iter()
                    .find(|p| p.id == pid)
                    .map(|p| html_escape(&p.name))
                    .unwrap_or_else(|| "(unknown)".to_string()),
                None => "All packages".to_string(),
            };

            let scope_display = acl
                .scope
                .as_deref()
                .map(html_escape)
                .unwrap_or_else(|| "\u{2014}".to_string());

            let grantee = if let Some(uid) = acl.user_id {
                let name = all_users
                    .iter()
                    .find(|u| u.id == uid)
                    .map(|u| html_escape(&u.username))
                    .unwrap_or_else(|| "(unknown user)".to_string());
                format!("User: {name}")
            } else if let Some(tid) = acl.team_id {
                let name = all_teams
                    .iter()
                    .find(|t| t.id == tid)
                    .map(|t| html_escape(&t.name))
                    .unwrap_or_else(|| "(unknown team)".to_string());
                format!("Team: {name}")
            } else {
                "\u{2014}".to_string()
            };

            let ts = acl.created_at.format("%Y-%m-%d").to_string();

            format!(
                r#"<tr>
  <td>{pkg_name}</td>
  <td>{scope_display}</td>
  <td>{grantee}</td>
  <td><span class="badge badge-outline">{permission}</span></td>
  <td class="text-sm opacity-60">{ts}</td>
  <td>
    <form method="post" action="/admin/access/{id}/revoke" class="inline">
      <button type="submit" class="btn btn-xs btn-error"
        onclick="return confirm('Revoke this ACL entry?')">Delete</button>
    </form>
  </td>
</tr>"#,
                id = acl.id,
                permission = html_escape(&acl.permission),
            )
        })
        .collect();

    let table = if rows.is_empty() {
        r#"<p class="opacity-60 mt-4">No ACL entries yet.</p>"#.to_string()
    } else {
        format!(
            r#"<div class="overflow-x-auto mb-6">
<table class="table table-zebra">
  <thead><tr>
    <th>Package</th>
    <th>Scope</th>
    <th>Grantee</th>
    <th>Permission</th>
    <th>Created</th>
    <th></th>
  </tr></thead>
  <tbody>{rows}</tbody>
</table>
</div>"#,
        )
    };

    // Package options for dropdown
    let package_options: String =
        std::iter::once(r#"<option value="">All packages</option>"#.to_string())
            .chain(all_packages.iter().map(|p| {
                format!(
                    r#"<option value="{id}">{name}</option>"#,
                    id = p.id,
                    name = html_escape(&p.name),
                )
            }))
            .collect();

    // User options
    let user_options: String = all_users
        .iter()
        .map(|u| {
            format!(
                r#"<option value="{id}">{name}</option>"#,
                id = u.id,
                name = html_escape(&u.username),
            )
        })
        .collect();

    // Team options
    let team_options: String = all_teams
        .iter()
        .map(|t| {
            format!(
                r#"<option value="{id}">{name}</option>"#,
                id = t.id,
                name = html_escape(&t.name),
            )
        })
        .collect();

    let create_form = format!(
        r#"<div class="card bg-base-200 shadow max-w-lg">
  <div class="card-body">
    <h2 class="card-title text-lg">Grant Access</h2>
    <form method="post" action="/admin/access" class="flex flex-col gap-3">
      <label class="form-control">
        <div class="label"><span class="label-text">Package</span></div>
        <select name="package_id" class="select select-bordered">
          {package_options}
        </select>
      </label>
      <label class="form-control">
        <div class="label"><span class="label-text">Scope (optional, e.g. @myorg)</span></div>
        <input type="text" name="scope" placeholder="@myorg"
          class="input input-bordered" />
      </label>
      <div class="form-control">
        <div class="label"><span class="label-text">Grant to</span></div>
        <div class="flex gap-4 items-center mb-2">
          <label class="label cursor-pointer gap-2">
            <input type="radio" name="grantee_type" value="user" class="radio radio-sm" checked />
            <span class="label-text">User</span>
          </label>
          <label class="label cursor-pointer gap-2">
            <input type="radio" name="grantee_type" value="team" class="radio radio-sm" />
            <span class="label-text">Team</span>
          </label>
        </div>
        <select name="user_id" class="select select-bordered mb-2">
          <option value="">-- select user --</option>
          {user_options}
        </select>
        <select name="team_id" class="select select-bordered">
          <option value="">-- select team --</option>
          {team_options}
        </select>
      </div>
      <label class="form-control">
        <div class="label"><span class="label-text">Permission</span></div>
        <select name="permission" class="select select-bordered" required>
          <option value="read">read</option>
          <option value="publish">publish</option>
          <option value="admin">admin</option>
        </select>
      </label>
      <button type="submit" class="btn btn-primary">Grant Access</button>
    </form>
  </div>
</div>"#,
    );

    let content = format!(
        "{heading}{table}{create_form}",
        heading = page_heading("Package Access Control"),
    );

    Ok(Html(layout("Access Control", &content)))
}

#[derive(Debug, Deserialize)]
pub struct GrantForm {
    pub package_id: Option<String>,
    pub scope: Option<String>,
    pub grantee_type: String,
    pub user_id: Option<String>,
    pub team_id: Option<String>,
    pub permission: String,
}

#[instrument(skip(state, form))]
pub async fn access_grant(
    State(state): State<AppState>,
    Form(form): Form<GrantForm>,
) -> WebResult<Redirect> {
    let db = &state.db;

    let package_id = form
        .package_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<Uuid>().ok());

    let scope = form
        .scope
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    let (user_id, team_id) = if form.grantee_type == "team" {
        let tid = form
            .team_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse::<Uuid>().ok());
        (None, tid)
    } else {
        let uid = form
            .user_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse::<Uuid>().ok());
        (uid, None)
    };

    AclRepo::grant(db, package_id, scope, user_id, team_id, &form.permission)
        .await
        .map_err(|e| crate::error::WebError::Internal(e.to_string()))?;

    Ok(Redirect::to("/admin/access"))
}

#[instrument(skip(state))]
pub async fn access_revoke(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> WebResult<Redirect> {
    let db = &state.db;

    AclRepo::revoke(db, id)
        .await
        .map_err(|e| crate::error::WebError::Internal(e.to_string()))?;

    Ok(Redirect::to("/admin/access"))
}
