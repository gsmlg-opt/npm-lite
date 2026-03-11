/// Render the full HTML layout with Tailwind CSS CDN and daisyUI CDN.
///
/// `title` is the page title; `content` is the inner HTML body.
pub fn layout(title: &str, content: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="dark">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>{title} — npm-lite</title>
  <link href="https://cdn.jsdelivr.net/npm/daisyui@4/dist/full.min.css" rel="stylesheet" />
  <script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="min-h-screen bg-base-100 text-base-content flex flex-col">

  <!-- Navbar -->
  <div class="navbar bg-base-300 shadow-md px-4">
    <div class="flex-1">
      <a href="/admin/" class="btn btn-ghost text-xl font-bold tracking-tight">
        📦 npm-lite
      </a>
    </div>
    <div class="flex-none">
      <ul class="menu menu-horizontal px-1 gap-1">
        <li><a href="/admin/" class="btn btn-ghost btn-sm">Dashboard</a></li>
        <li><a href="/admin/packages" class="btn btn-ghost btn-sm">Packages</a></li>
        <li><a href="/admin/tokens" class="btn btn-ghost btn-sm">Tokens</a></li>
        <li><a href="/admin/teams" class="btn btn-ghost btn-sm">Teams</a></li>
        <li><a href="/admin/activity" class="btn btn-ghost btn-sm">Activity</a></li>
        <li><a href="/admin/login" class="btn btn-outline btn-sm btn-primary">Login</a></li>
      </ul>
    </div>
  </div>

  <!-- Page content -->
  <main class="flex-1 container mx-auto px-4 py-8 max-w-7xl">
    {content}
  </main>

  <!-- Footer -->
  <footer class="footer footer-center p-4 bg-base-300 text-base-content text-sm">
    <p>npm-lite registry — self-hosted npm package registry</p>
  </footer>

</body>
</html>"#,
        title = html_escape(title),
        content = content,
    )
}

/// Minimal HTML entity escaping for use in attribute values and text nodes.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Render an alert/flash message box.
pub fn alert(kind: &str, message: &str) -> String {
    let class = match kind {
        "success" => "alert-success",
        "error" => "alert-error",
        "warning" => "alert-warning",
        _ => "alert-info",
    };
    format!(
        r#"<div class="alert {class} mb-4" role="alert"><span>{message}</span></div>"#,
        class = class,
        message = html_escape(message),
    )
}

/// Render a page heading.
pub fn page_heading(title: &str) -> String {
    format!(
        r#"<h1 class="text-3xl font-bold mb-6">{}</h1>"#,
        html_escape(title)
    )
}

/// Render a stat card for the dashboard.
pub fn stat_card(label: &str, value: &str, desc: &str) -> String {
    format!(
        r#"<div class="stat bg-base-200 rounded-box shadow">
  <div class="stat-title">{label}</div>
  <div class="stat-value text-primary">{value}</div>
  <div class="stat-desc">{desc}</div>
</div>"#,
        label = html_escape(label),
        value = html_escape(value),
        desc = html_escape(desc),
    )
}
