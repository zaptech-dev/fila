use bytes::Bytes;
use http_body_util::Full;
use rapina::database::{Db, DbError};
use rapina::http::Response;
use rapina::http::header::CONTENT_TYPE;
use rapina::prelude::*;
use rapina::response::BoxBody;
use rapina::sea_orm::{
    ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};

use crate::entity::PullRequest;
use crate::entity::pull_request::Column as PrColumn;
use crate::types::PrStatus;

const PAGE_SIZE: u64 = 50;

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(serde::Deserialize)]
struct DashboardQuery {
    page: Option<u64>,
}

#[public]
#[get("/")]
pub async fn dashboard(db: Db, query: Query<DashboardQuery>) -> Result<Response<BoxBody>> {
    let page = query.page.unwrap_or(1).max(1);
    let now = chrono::Utc::now();

    // Stats
    let total = PullRequest::find()
        .count(db.conn())
        .await
        .map_err(DbError)?;

    let in_queue = PullRequest::find()
        .filter(
            PrColumn::Status
                .eq(PrStatus::Queued.as_ref())
                .or(PrColumn::Status.eq(PrStatus::Testing.as_ref()))
                .or(PrColumn::Status.eq(PrStatus::Batched.as_ref())),
        )
        .count(db.conn())
        .await
        .map_err(DbError)?;

    let failed = PullRequest::find()
        .filter(PrColumn::Status.eq(PrStatus::Failed.as_ref()))
        .count(db.conn())
        .await
        .map_err(DbError)?;

    let merged = PullRequest::find()
        .filter(PrColumn::Status.eq(PrStatus::Merged.as_ref()))
        .count(db.conn())
        .await
        .map_err(DbError)?;

    // All PRs: active first (queued/testing/batched), then rest by id desc
    // SeaORM doesn't support CASE ordering easily, so we do two queries
    let active_statuses = vec![
        PrStatus::Queued.to_string(),
        PrStatus::Testing.to_string(),
        PrStatus::Batched.to_string(),
    ];

    let active_prs = PullRequest::find()
        .filter(PrColumn::Status.is_in(active_statuses))
        .order_by_desc(PrColumn::Priority)
        .order_by_asc(PrColumn::QueuedAt)
        .all(db.conn())
        .await
        .map_err(DbError)?;

    let inactive_statuses = vec![
        PrStatus::Merged.to_string(),
        PrStatus::Failed.to_string(),
        PrStatus::Cancelled.to_string(),
    ];

    let offset = (page - 1) * PAGE_SIZE;
    let inactive_prs = PullRequest::find()
        .filter(PrColumn::Status.is_in(inactive_statuses))
        .order_by_desc(PrColumn::Id)
        .offset(offset)
        .limit(PAGE_SIZE)
        .all(db.conn())
        .await
        .map_err(DbError)?;

    let inactive_total = PullRequest::find()
        .filter(
            PrColumn::Status
                .eq(PrStatus::Merged.as_ref())
                .or(PrColumn::Status.eq(PrStatus::Failed.as_ref()))
                .or(PrColumn::Status.eq(PrStatus::Cancelled.as_ref())),
        )
        .count(db.conn())
        .await
        .map_err(DbError)?;

    let total_pages = (inactive_total as f64 / PAGE_SIZE as f64).ceil() as u64;

    let mut html = String::with_capacity(32768);
    html.push_str(HEADER);

    // Summary stats
    html.push_str(&format!(
        "<div id=\"section-stats\" class=\"stats\">{} total &middot; {} in queue &middot; {} merged &middot; {} failed</div>",
        total, in_queue, merged, failed,
    ));

    // PR table
    html.push_str("<div id=\"section-prs\">");
    if active_prs.is_empty() && inactive_prs.is_empty() {
        html.push_str("<p class=\"empty\">No PRs yet.</p>");
    } else {
        html.push_str(
            "<table><thead><tr><th>#</th><th>Status</th><th>Title</th><th>Author</th><th>Shipped by</th><th>HEAD</th><th>Time</th></tr></thead><tbody>",
        );

        for pr in active_prs.iter().chain(inactive_prs.iter()) {
            let short_sha = &pr.head_sha[..7.min(pr.head_sha.len())];
            let approved = pr.approved_by.as_deref().unwrap_or("\u{2014}");
            let status = PrStatus::from(pr.status.as_str());
            let time_col = match status {
                PrStatus::Queued | PrStatus::Testing | PrStatus::Batched => pr
                    .queued_at
                    .map(|t| relative_time(now, t))
                    .unwrap_or_default(),
                PrStatus::Merged => pr
                    .merged_at
                    .map(|t| relative_time(now, t))
                    .unwrap_or_default(),
                _ => pr
                    .queued_at
                    .map(|t| relative_time(now, t))
                    .unwrap_or_default(),
            };

            html.push_str(&format!(
                "<tr><td class=\"mono\"><a href=\"https://github.com/{}/{}/pull/{}\" target=\"_blank\">{}</a></td><td><span class=\"status status-{}\">{}</span></td><td>{}</td><td>{}</td><td>{}</td><td class=\"mono\"><a href=\"https://github.com/{}/{}/commit/{}\" target=\"_blank\">{}</a></td><td class=\"mono\">{}</td></tr>",
                pr.repo_owner, pr.repo_name, pr.pr_number, pr.pr_number,
                pr.status, pr.status,
                escape_html(&pr.title), escape_html(&pr.author), escape_html(approved),
                pr.repo_owner, pr.repo_name, pr.head_sha, short_sha,
                time_col,
            ));
        }
        html.push_str("</tbody></table>");

        // Pagination
        if total_pages > 1 {
            html.push_str("<div class=\"pagination\">");
            if page > 1 {
                html.push_str(&format!(
                    "<a href=\"/?page={}\">&#8592; Newer</a>",
                    page - 1
                ));
            }
            html.push_str(&format!(
                "<span class=\"page-info\">Page {} of {}</span>",
                page, total_pages
            ));
            if page < total_pages {
                html.push_str(&format!(
                    "<a href=\"/?page={}\">Older &#8594;</a>",
                    page + 1
                ));
            }
            html.push_str("</div>");
        }
    }
    html.push_str("</div>");

    html.push_str(FOOTER);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(html)))
        .unwrap())
}

fn relative_time(
    now: chrono::DateTime<chrono::Utc>,
    then: chrono::DateTime<chrono::Utc>,
) -> String {
    let secs = (now - then).num_seconds();
    if secs < 0 {
        return "just now".to_string();
    }
    let secs = secs as u64;
    match secs {
        0..=59 => format!("{}s ago", secs),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86399 => {
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            if m == 0 {
                format!("{}h ago", h)
            } else {
                format!("{}h {}m ago", h, m)
            }
        }
        _ => format!("{}d ago", secs / 86400),
    }
}

const HEADER: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Fila — Merge Queue</title>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif; background: #fff; color: #1a1a1a; padding: 2rem; max-width: 1200px; margin: 0 auto; }
h1 { font-size: 1.5rem; margin-bottom: 0.5rem; font-weight: 600; }
h2 { font-size: 1.1rem; margin: 2rem 0 0.75rem; font-weight: 600; color: #333; }
.stats { font-size: 0.875rem; color: #666; margin-bottom: 1.5rem; }
table { width: 100%; border-collapse: collapse; margin-bottom: 1rem; font-size: 0.875rem; }
th { text-align: left; padding: 0.5rem 0.75rem; border-bottom: 2px solid #e5e5e5; font-weight: 600; color: #666; }
td { padding: 0.5rem 0.75rem; border-bottom: 1px solid #f0f0f0; }
tr:hover td { background: #fafafa; }
.mono { font-family: "SF Mono", "Fira Code", "Fira Mono", Menlo, monospace; font-size: 0.8125rem; }
.status { display: inline-block; padding: 0.125rem 0.5rem; border-radius: 3px; font-size: 0.75rem; font-weight: 500; white-space: nowrap; }
.status-queued { background: #dbeafe; color: #1e40af; }
.status-batched { background: #fef3c7; color: #92400e; }
.status-merged { background: #d1fae5; color: #065f46; }
.status-failed { background: #fee2e2; color: #991b1b; }
.status-cancelled { background: #f3f4f6; color: #6b7280; }
.status-pending { background: #e0e7ff; color: #3730a3; }
.status-testing { background: #fef3c7; color: #92400e; }
.status-done { background: #d1fae5; color: #065f46; }
a { color: #1e40af; text-decoration: none; }
a:hover { text-decoration: underline; }
.empty { color: #999; font-style: italic; padding: 1rem 0; }
.updated { font-size: 0.75rem; color: #999; margin-top: 1rem; }
.pagination { display: flex; align-items: center; gap: 1rem; font-size: 0.875rem; margin-bottom: 1rem; }
.page-info { color: #999; }
</style>
</head>
<body>
<h1>Fila &mdash; Merge Queue</h1>
"#;

const FOOTER: &str = r#"<p class="updated" id="updated"></p>
<script>
(function() {
  var ids = ['section-stats', 'section-prs'];
  function refresh() {
    fetch(window.location.href, { headers: { 'Accept': 'text/html' } })
      .then(function(r) { return r.text(); })
      .then(function(html) {
        var doc = new DOMParser().parseFromString(html, 'text/html');
        ids.forEach(function(id) {
          var fresh = doc.getElementById(id);
          var current = document.getElementById(id);
          if (fresh && current) current.innerHTML = fresh.innerHTML;
        });
        document.getElementById('updated').textContent = 'Updated ' + new Date().toLocaleTimeString();
      })
      .catch(function() {});
  }
  setInterval(refresh, 5000);
})();
</script>
</body>
</html>"#;
