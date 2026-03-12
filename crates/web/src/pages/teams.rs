use axum::{
    Form,
    extract::{Path, State},
    response::{Html, Redirect},
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use npm_entity::{team_members, teams, users};

use crate::{
    error::{WebError, WebResult},
    state::AppState,
    templates::{html_escape, layout, page_heading},
};

#[instrument(skip(state))]
pub async fn team_list_page(State(state): State<AppState>) -> WebResult<Html<String>> {
    let db = &state.db;

    let all_teams = teams::Entity::find()
        .order_by_asc(teams::Column::Name)
        .all(db)
        .await?;

    let rows: String = all_teams
        .iter()
        .map(|t| {
            let desc = t.description.as_deref().unwrap_or("—");
            let ts = t.created_at.format("%Y-%m-%d").to_string();
            format!(
                r#"<tr>
  <td>
    <a href="/admin/teams/{id}" class="link link-primary font-semibold">{name}</a>
  </td>
  <td class="text-sm opacity-80">{desc}</td>
  <td class="text-sm opacity-60">{ts}</td>
</tr>"#,
                id = t.id,
                name = html_escape(&t.name),
                desc = html_escape(desc),
            )
        })
        .collect();

    let table = if rows.is_empty() {
        r#"<p class="opacity-60 mt-4">No teams yet.</p>"#.to_string()
    } else {
        format!(
            r#"<div class="overflow-x-auto mb-6">
<table class="table table-zebra">
  <thead><tr><th>Name</th><th>Description</th><th>Created</th></tr></thead>
  <tbody>{rows}</tbody>
</table>
</div>"#,
        )
    };

    let create_form = r#"<div class="card bg-base-200 shadow max-w-md">
  <div class="card-body">
    <h2 class="card-title text-lg">Create Team</h2>
    <form method="post" action="/admin/teams" class="flex flex-col gap-3">
      <label class="form-control">
        <div class="label"><span class="label-text">Team Name</span></div>
        <input type="text" name="name" required placeholder="platform-team"
          class="input input-bordered" />
      </label>
      <label class="form-control">
        <div class="label"><span class="label-text">Description (optional)</span></div>
        <input type="text" name="description" placeholder="Platform infrastructure team"
          class="input input-bordered" />
      </label>
      <button type="submit" class="btn btn-primary">Create Team</button>
    </form>
  </div>
</div>"#;

    let content = format!(
        "{heading}{table}{create_form}",
        heading = page_heading("Teams"),
    );

    Ok(Html(layout("Teams", &content)))
}

#[derive(Debug, Deserialize)]
pub struct CreateTeamForm {
    pub name: String,
    pub description: Option<String>,
}

#[instrument(skip(state, form))]
pub async fn team_create(
    State(state): State<AppState>,
    Form(form): Form<CreateTeamForm>,
) -> WebResult<Redirect> {
    use chrono::Utc;

    let db = &state.db;

    let model = teams::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(form.name.trim().to_string()),
        description: Set(form
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)),
        created_at: Set(Utc::now().into()),
    };

    model.insert(db).await?;

    Ok(Redirect::to("/admin/teams"))
}

#[instrument(skip(state))]
pub async fn team_detail_page(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> WebResult<Html<String>> {
    let db = &state.db;

    let team = teams::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(WebError::NotFound)?;

    // Get members with user info
    let members_with_users: Vec<(team_members::Model, Option<users::Model>)> =
        team_members::Entity::find()
            .filter(team_members::Column::TeamId.eq(team.id))
            .find_also_related(users::Entity)
            .order_by_asc(team_members::Column::CreatedAt)
            .all(db)
            .await?;

    // All users for the add-member dropdown
    let all_users = users::Entity::find()
        .order_by_asc(users::Column::Username)
        .all(db)
        .await?;

    let desc = team.description.as_deref().unwrap_or("No description.");

    let member_rows: String = members_with_users
        .iter()
        .map(|(m, u)| {
            let username = u
                .as_ref()
                .map(|u| u.username.as_str())
                .unwrap_or("(deleted)");
            let since = m.created_at.format("%Y-%m-%d").to_string();
            format!(
                r#"<tr>
  <td class="font-mono">{username}</td>
  <td class="text-sm opacity-60">{since}</td>
  <td>
    <form method="post" action="/admin/teams/{tid}/members/{mid}/remove" class="inline">
      <button type="submit" class="btn btn-xs btn-error"
        onclick="return confirm('Remove member?')">Remove</button>
    </form>
  </td>
</tr>"#,
                username = html_escape(username),
                tid = team.id,
                mid = m.id,
            )
        })
        .collect();

    let member_table = if member_rows.is_empty() {
        r#"<p class="opacity-60 mb-4">No members yet.</p>"#.to_string()
    } else {
        format!(
            r#"<div class="overflow-x-auto mb-4">
<table class="table table-zebra">
  <thead><tr><th>Username</th><th>Member since</th><th></th></tr></thead>
  <tbody>{member_rows}</tbody>
</table>
</div>"#,
        )
    };

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

    let add_member_form = format!(
        r#"<form method="post" action="/admin/teams/{id}/members" class="flex gap-2 items-end flex-wrap">
  <label class="form-control">
    <div class="label"><span class="label-text">Add member</span></div>
    <select name="user_id" class="select select-bordered select-sm">
      {user_options}
    </select>
  </label>
  <button type="submit" class="btn btn-sm btn-secondary">Add</button>
</form>"#,
        id = team.id,
    );

    let content = format!(
        r#"<div class="mb-2 text-sm breadcrumbs">
  <ul><li><a href="/admin/teams">Teams</a></li><li>{name}</li></ul>
</div>
<h1 class="text-3xl font-bold mb-1">{name}</h1>
<p class="text-base opacity-70 mb-6">{desc}</p>
<div class="card bg-base-200 shadow mb-6">
  <div class="card-body">
    <h2 class="card-title text-lg mb-2">Members</h2>
    {member_table}
    {add_member_form}
  </div>
</div>"#,
        name = html_escape(&team.name),
        desc = html_escape(desc),
    );

    Ok(Html(layout(&team.name, &content)))
}

#[derive(Debug, Deserialize)]
pub struct AddMemberForm {
    pub user_id: Uuid,
}

#[instrument(skip(state, form))]
pub async fn team_add_member(
    State(state): State<AppState>,
    Path(team_id): Path<Uuid>,
    Form(form): Form<AddMemberForm>,
) -> WebResult<Redirect> {
    use chrono::Utc;

    let db = &state.db;

    // Check team exists
    teams::Entity::find_by_id(team_id)
        .one(db)
        .await?
        .ok_or(WebError::NotFound)?;

    // Avoid duplicate membership
    let existing = team_members::Entity::find()
        .filter(team_members::Column::TeamId.eq(team_id))
        .filter(team_members::Column::UserId.eq(form.user_id))
        .one(db)
        .await?;

    if existing.is_none() {
        let model = team_members::ActiveModel {
            id: Set(Uuid::new_v4()),
            team_id: Set(team_id),
            user_id: Set(form.user_id),
            created_at: Set(Utc::now().into()),
        };
        model.insert(db).await?;
    }

    Ok(Redirect::to(&format!("/admin/teams/{team_id}")))
}

#[instrument(skip(state))]
pub async fn team_remove_member(
    State(state): State<AppState>,
    Path((team_id, member_id)): Path<(Uuid, Uuid)>,
) -> WebResult<Redirect> {
    use sea_orm::ModelTrait;

    let db = &state.db;

    let member = team_members::Entity::find_by_id(member_id)
        .one(db)
        .await?
        .ok_or(WebError::NotFound)?;

    // Validate that the member actually belongs to the specified team.
    if member.team_id != team_id {
        return Err(WebError::NotFound);
    }

    member.delete(db).await?;

    Ok(Redirect::to(&format!("/admin/teams/{team_id}")))
}
