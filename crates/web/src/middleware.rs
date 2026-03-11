use axum::{
    extract::Request,
    http::header,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};

pub async fn require_admin_session(
    request: Request,
    next: Next,
) -> Response {
    let has_session = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|cookies| cookies.contains("admin_user="))
        .unwrap_or(false);

    if !has_session {
        return Redirect::to("/admin/login").into_response();
    }

    next.run(request).await
}
