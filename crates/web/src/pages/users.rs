use axum::{
    Form,
    extract::State,
    response::{Html, Redirect},
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, QueryOrder};
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use npm_entity::users;

use crate::{
    error::WebResult,
    state::AppState,
    templates::{html_escape, layout, page_heading},
};

#[instrument(skip(state))]
pub async fn user_list_page(State(state): State<AppState>) -> WebResult<Html<String>> {
    let db = &state.db;

    let all_users = users::Entity::find()
        .order_by_asc(users::Column::Username)
        .all(db)
        .await?;

    let rows: String = all_users
        .iter()
        .map(|u| {
            let role_class = match u.role.as_str() {
                "admin" => "badge-error",
                "publish" => "badge-warning",
                _ => "badge-info",
            };
            let ts = u.created_at.format("%Y-%m-%d").to_string();
            format!(
                r#"<tr>
  <td class="font-mono">{username}</td>
  <td>{email}</td>
  <td><span class="badge {role_class} badge-sm">{role}</span></td>
  <td class="text-sm opacity-60">{ts}</td>
</tr>"#,
                username = html_escape(&u.username),
                email = html_escape(&u.email),
                role = html_escape(&u.role),
            )
        })
        .collect();

    let table = if rows.is_empty() {
        r#"<p class="opacity-60 mt-4">No users yet.</p>"#.to_string()
    } else {
        format!(
            r#"<div class="overflow-x-auto mb-6">
<table class="table table-zebra">
  <thead><tr><th>Username</th><th>Email</th><th>Role</th><th>Created</th></tr></thead>
  <tbody>{rows}</tbody>
</table>
</div>"#,
        )
    };

    let create_form = r#"<div class="card bg-base-200 shadow max-w-md">
  <div class="card-body">
    <h2 class="card-title text-lg">Create User</h2>
    <form method="post" action="/admin/users" class="flex flex-col gap-3">
      <label class="form-control">
        <div class="label"><span class="label-text">Username</span></div>
        <input type="text" name="username" required placeholder="alice"
          class="input input-bordered" />
      </label>
      <label class="form-control">
        <div class="label"><span class="label-text">Email</span></div>
        <input type="email" name="email" required placeholder="alice@example.com"
          class="input input-bordered" />
      </label>
      <label class="form-control">
        <div class="label"><span class="label-text">Password</span></div>
        <input type="password" name="password" required minlength="6"
          class="input input-bordered" />
      </label>
      <label class="form-control">
        <div class="label"><span class="label-text">Role</span></div>
        <select name="role" class="select select-bordered">
          <option value="read">read</option>
          <option value="publish" selected>publish</option>
          <option value="admin">admin</option>
        </select>
      </label>
      <button type="submit" class="btn btn-primary">Create User</button>
    </form>
  </div>
</div>"#;

    let content = format!(
        "{heading}{table}{create_form}",
        heading = page_heading("Users"),
    );

    Ok(Html(layout("Users", &content)))
}

#[derive(Debug, Deserialize)]
pub struct CreateUserForm {
    pub username: String,
    pub email: String,
    pub password: String,
    pub role: String,
}

#[instrument(skip(state, form), fields(username = %form.username))]
pub async fn user_create(
    State(state): State<AppState>,
    Form(form): Form<CreateUserForm>,
) -> WebResult<Redirect> {
    let db = &state.db;

    let password_hash = npm_core::auth::hash_password(&form.password)
        .map_err(|e| crate::error::WebError::Internal(e.to_string()))?;

    let role = match form.role.as_str() {
        "read" | "publish" | "admin" => form.role,
        _ => "read".to_string(),
    };

    let now = chrono::Utc::now().fixed_offset();
    let model = users::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(form.username.trim().to_string()),
        password_hash: Set(password_hash),
        email: Set(form.email.trim().to_string()),
        role: Set(role),
        created_at: Set(now),
        updated_at: Set(now),
    };

    model.insert(db).await?;

    tracing::info!(username = %form.username, "user created via admin UI");

    Ok(Redirect::to("/admin/users"))
}
