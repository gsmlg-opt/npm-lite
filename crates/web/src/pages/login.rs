use axum::{
    Form,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use tracing::instrument;

use npm_entity::users;

use crate::{
    error::WebResult,
    state::AppState,
    templates::{alert, html_escape, layout},
};

pub async fn login_page() -> Html<String> {
    Html(layout("Login", &login_form(None)))
}

fn login_form(error: Option<&str>) -> String {
    let error_html = error.map(|e| alert("error", e)).unwrap_or_default();

    format!(
        r#"{error_html}
<div class="flex justify-center items-center min-h-[60vh]">
  <div class="card bg-base-200 shadow-xl w-full max-w-sm">
    <div class="card-body">
      <h2 class="card-title text-2xl justify-center mb-4">Admin Login</h2>
      <form method="post" action="/admin/login" class="flex flex-col gap-4">
        <label class="form-control">
          <div class="label"><span class="label-text">Username</span></div>
          <input type="text" name="username" required autofocus
            class="input input-bordered" placeholder="admin" />
        </label>
        <label class="form-control">
          <div class="label"><span class="label-text">Password</span></div>
          <input type="password" name="password" required
            class="input input-bordered" placeholder="••••••••" />
        </label>
        <button type="submit" class="btn btn-primary w-full">Sign in</button>
      </form>
    </div>
  </div>
</div>"#,
    )
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[instrument(skip(state, form), fields(username = %form.username))]
pub async fn login_post(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> WebResult<Response> {
    use npm_core::auth::verify_password;

    let db = &state.db;

    let user = users::Entity::find()
        .filter(users::Column::Username.eq(&form.username))
        .one(db)
        .await?;

    let authed = match &user {
        Some(u) if u.role == "admin" => {
            verify_password(&form.password, &u.password_hash).unwrap_or(false)
        }
        _ => false,
    };

    if !authed {
        tracing::warn!(username = %form.username, "failed admin login attempt");
        let body = Html(layout("Login", &login_form(Some("Invalid credentials."))));
        return Ok((StatusCode::UNAUTHORIZED, body).into_response());
    }

    tracing::info!(username = %form.username, "admin login successful");

    // Set a simple session cookie containing the username.
    // In production, use a signed/encrypted cookie or proper session store.
    let cookie_value = format!(
        "admin_user={}; HttpOnly; Secure; SameSite=Lax; Path=/admin",
        html_escape(&form.username),
    );

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::LOCATION, HeaderValue::from_static("/admin/"));
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie_value).unwrap_or(HeaderValue::from_static("/")),
    );

    Ok((StatusCode::SEE_OTHER, headers).into_response())
}

pub async fn logout() -> Response {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::LOCATION, HeaderValue::from_static("/admin/login"));
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_static("admin_user=; HttpOnly; SameSite=Lax; Path=/admin; Max-Age=0"),
    );
    (StatusCode::SEE_OTHER, headers).into_response()
}
