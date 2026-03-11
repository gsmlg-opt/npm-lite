use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
    Form,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, QueryOrder};
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use npm_entity::tokens;

use crate::{
    error::{WebError, WebResult},
    state::AppState,
    templates::{html_escape, layout, page_heading},
};

#[instrument(skip(state))]
pub async fn token_list_page(State(state): State<AppState>) -> WebResult<Html<String>> {
    let db = &state.db;

    let all_tokens = tokens::Entity::find()
        .order_by_desc(tokens::Column::CreatedAt)
        .all(db)
        .await?;

    let rows: String = all_tokens
        .iter()
        .map(|t| {
            let name = t.name.as_deref().unwrap_or("(unnamed)");
            let revoked = t
                .revoked_at
                .as_ref()
                .map(|ts| {
                    format!(
                        r#"<span class="badge badge-error badge-sm">revoked {}</span>"#,
                        ts.format("%Y-%m-%d"),
                    )
                })
                .unwrap_or_else(|| {
                    r#"<span class="badge badge-success badge-sm">active</span>"#.to_string()
                });
            let ts = t.created_at.format("%Y-%m-%d %H:%M UTC").to_string();
            let revoke_btn = if t.revoked_at.is_none() {
                format!(
                    r#"<form method="post" action="/admin/tokens/{id}/revoke" class="inline">
  <button type="submit" class="btn btn-xs btn-error"
    onclick="return confirm('Revoke this token?')">Revoke</button>
</form>"#,
                    id = t.id,
                )
            } else {
                String::new()
            };
            format!(
                r#"<tr>
  <td class="font-mono text-sm">{name}</td>
  <td><span class="badge badge-outline badge-sm">{role}</span></td>
  <td>{revoked}</td>
  <td class="text-sm opacity-60">{ts}</td>
  <td>{revoke_btn}</td>
</tr>"#,
                name = html_escape(name),
                role = html_escape(&t.role),
            )
        })
        .collect();

    let table = if rows.is_empty() {
        r#"<p class="opacity-60 mt-4">No tokens yet.</p>"#.to_string()
    } else {
        format!(
            r#"<div class="overflow-x-auto mb-6">
<table class="table table-zebra">
  <thead><tr><th>Name</th><th>Role</th><th>Status</th><th>Created</th><th></th></tr></thead>
  <tbody>{rows}</tbody>
</table>
</div>"#,
        )
    };

    let create_form = r#"<div class="card bg-base-200 shadow max-w-md">
  <div class="card-body">
    <h2 class="card-title text-lg">Create Token</h2>
    <form method="post" action="/admin/tokens" class="flex flex-col gap-3">
      <label class="form-control">
        <div class="label"><span class="label-text">Name (optional)</span></div>
        <input type="text" name="name" placeholder="CI/CD token" class="input input-bordered" />
      </label>
      <label class="form-control">
        <div class="label"><span class="label-text">Role</span></div>
        <select name="role" class="select select-bordered">
          <option value="read">read</option>
          <option value="publish" selected>publish</option>
          <option value="admin">admin</option>
        </select>
      </label>
      <button type="submit" class="btn btn-primary">Create Token</button>
    </form>
  </div>
</div>"#;

    let content = format!(
        "{heading}{table}{create_form}",
        heading = page_heading("Access Tokens"),
    );

    Ok(Html(layout("Tokens", &content)))
}

#[derive(Debug, Deserialize)]
pub struct CreateTokenForm {
    pub name: Option<String>,
    pub role: String,
}

#[instrument(skip(state, form))]
pub async fn token_create(
    State(state): State<AppState>,
    Form(form): Form<CreateTokenForm>,
) -> WebResult<Redirect> {
    use chrono::Utc;
    use npm_core::auth::generate_token;

    let db = &state.db;

    let raw_token = generate_token();
    let token_hash = npm_core::auth::hash_token(&raw_token);

    // Use a placeholder user id (admin user should exist from setup)
    // In a real implementation, derive from session cookie.
    let placeholder_user_id = Uuid::nil();

    let name = form
        .name
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    let role = match form.role.as_str() {
        "read" | "publish" | "admin" => form.role,
        _ => "publish".to_string(),
    };

    let model = tokens::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(placeholder_user_id),
        token_hash: Set(token_hash),
        role: Set(role),
        name: Set(name),
        created_at: Set(Utc::now().into()),
        revoked_at: Set(None),
    };

    model.insert(db).await?;

    tracing::info!(token_prefix = &raw_token[..8], "token created");

    Ok(Redirect::to("/admin/tokens"))
}

#[instrument(skip(state))]
pub async fn token_revoke(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> WebResult<Redirect> {
    use chrono::Utc;

    let db = &state.db;

    let token = tokens::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(WebError::NotFound)?;

    let mut active: tokens::ActiveModel = token.into();
    active.revoked_at = Set(Some(Utc::now().into()));
    active.update(db).await?;

    Ok(Redirect::to("/admin/tokens"))
}
