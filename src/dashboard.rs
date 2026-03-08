use bytes::Bytes;
use http_body_util::Full;
use rapina::database::{Db, DbError};
use rapina::http::Response;
use rapina::http::header::CONTENT_TYPE;
use rapina::prelude::*;
use rapina::response::BoxBody;
use rapina::sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::entity::batch::Column as BatchColumn;
use crate::entity::merge_event::Column as EventColumn;
use crate::entity::pull_request::Column as PrColumn;
use crate::entity::{Batch, MergeEvent, PullRequest};
use crate::types::PrStatus;

#[public]
#[get("/")]
pub async fn dashboard(db: Db) -> Result<Response<BoxBody>> {
    let queued = PullRequest::find()
        .filter(PrColumn::Status.eq(PrStatus::Queued.as_ref()))
        .order_by_desc(PrColumn::Priority)
        .order_by_asc(PrColumn::QueuedAt)
        .all(db.conn())
        .await
        .map_err(DbError)?;

    let testing = PullRequest::find()
        .filter(PrColumn::Status.eq(PrStatus::Testing.as_ref()))
        .order_by_asc(PrColumn::QueuedAt)
        .all(db.conn())
        .await
        .map_err(DbError)?;

    let batches = Batch::find()
        .order_by_desc(BatchColumn::Id)
        .limit(10)
        .all(db.conn())
        .await
        .map_err(DbError)?;

    let events = MergeEvent::find()
        .order_by_desc(EventColumn::Id)
        .limit(20)
        .all(db.conn())
        .await
        .map_err(DbError)?;

    let mut html = String::with_capacity(16384);
    html.push_str(HEADER);

    // Queue section
    html.push_str("<h2>Queue</h2>");
    if queued.is_empty() {
        html.push_str("<p class=\"empty\">No PRs in queue.</p>");
    } else {
        html.push_str(
            "<table><thead><tr><th>PR</th><th>Title</th><th>Author</th><th>Shipped by</th><th>HEAD</th><th>Status</th><th>Time in queue</th></tr></thead><tbody>",
        );
        let now = chrono::Utc::now();
        for pr in &queued {
            let time_in_queue = pr
                .queued_at
                .map(|t| relative_time(now, t))
                .unwrap_or_default();
            let short_sha = &pr.head_sha[..7.min(pr.head_sha.len())];
            let approved = pr.approved_by.as_deref().unwrap_or("\u{2014}");
            html.push_str(&format!(
                "<tr><td class=\"mono\">#{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"mono\">{}</td><td><span class=\"status status-queued\">queued</span></td><td class=\"mono\">{}</td></tr>",
                pr.pr_number, pr.title, pr.author, approved, short_sha, time_in_queue,
            ));
        }
        html.push_str("</tbody></table>");
    }

    // In Progress section
    html.push_str("<h2>In Progress</h2>");
    if testing.is_empty() {
        html.push_str("<p class=\"empty\">No PRs currently testing.</p>");
    } else {
        html.push_str(
            "<table><thead><tr><th>PR</th><th>Title</th><th>Author</th><th>Shipped by</th><th>HEAD</th><th>Status</th></tr></thead><tbody>",
        );
        for pr in &testing {
            let short_sha = &pr.head_sha[..7.min(pr.head_sha.len())];
            let approved = pr.approved_by.as_deref().unwrap_or("\u{2014}");
            html.push_str(&format!(
                "<tr><td class=\"mono\">#{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"mono\">{}</td><td><span class=\"status status-testing\">testing</span></td></tr>",
                pr.pr_number, pr.title, pr.author, approved, short_sha,
            ));
        }
        html.push_str("</tbody></table>");
    }

    // Batches section
    html.push_str("<h2>Recent Batches</h2>");
    if batches.is_empty() {
        html.push_str("<p class=\"empty\">No batches yet.</p>");
    } else {
        html.push_str(
            "<table><thead><tr><th>ID</th><th>Status</th><th>Completed At</th></tr></thead><tbody>",
        );
        for batch in &batches {
            html.push_str(&format!(
                "<tr><td class=\"mono\">{}</td><td><span class=\"status status-{}\">{}</span></td><td class=\"mono\">{}</td></tr>",
                batch.id,
                batch.status,
                batch.status,
                batch.completed_at.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_else(|| "\u{2014}".to_string()),
            ));
        }
        html.push_str("</tbody></table>");
    }

    // Recent Activity section
    html.push_str("<h2>Recent Activity</h2>");
    if events.is_empty() {
        html.push_str("<p class=\"empty\">No events yet.</p>");
    } else {
        html.push_str(
            "<table><thead><tr><th>Event</th><th>PR ID</th><th>Batch ID</th><th>Details</th></tr></thead><tbody>",
        );
        for event in &events {
            html.push_str(&format!(
                "<tr><td class=\"mono\">{}</td><td class=\"mono\">{}</td><td class=\"mono\">{}</td><td>{}</td></tr>",
                event.event_type,
                event.pull_request_id,
                event.batch_id,
                event.details.as_deref().unwrap_or("\u{2014}"),
            ));
        }
        html.push_str("</tbody></table>");
    }

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
h1 { font-size: 1.5rem; margin-bottom: 2rem; font-weight: 600; }
h2 { font-size: 1.1rem; margin: 2rem 0 0.75rem; font-weight: 600; color: #333; }
table { width: 100%; border-collapse: collapse; margin-bottom: 1.5rem; font-size: 0.875rem; }
th { text-align: left; padding: 0.5rem 0.75rem; border-bottom: 2px solid #e5e5e5; font-weight: 600; color: #666; }
td { padding: 0.5rem 0.75rem; border-bottom: 1px solid #f0f0f0; }
tr:hover td { background: #fafafa; }
.mono { font-family: "SF Mono", "Fira Code", "Fira Mono", Menlo, monospace; font-size: 0.8125rem; }
.status { display: inline-block; padding: 0.125rem 0.5rem; border-radius: 3px; font-size: 0.75rem; font-weight: 500; }
.status-queued { background: #dbeafe; color: #1e40af; }
.status-batched { background: #fef3c7; color: #92400e; }
.status-merged { background: #d1fae5; color: #065f46; }
.status-failed { background: #fee2e2; color: #991b1b; }
.status-cancelled { background: #f3f4f6; color: #6b7280; }
.status-pending { background: #e0e7ff; color: #3730a3; }
.status-testing { background: #fef3c7; color: #92400e; }
.status-done { background: #d1fae5; color: #065f46; }
.empty { color: #999; font-style: italic; padding: 1rem 0; }
.updated { font-size: 0.75rem; color: #999; margin-top: 2rem; }
</style>
</head>
<body>
<h1>Fila &mdash; Merge Queue</h1>
"#;

const FOOTER: &str = r#"<p class="updated" id="updated"></p>
<script>
(function() {
  var interval = 5000;
  function refresh() {
    fetch(window.location.href, { headers: { 'Accept': 'text/html' } })
      .then(function(r) { return r.text(); })
      .then(function(html) {
        var doc = new DOMParser().parseFromString(html, 'text/html');
        var sections = doc.querySelectorAll('h2, table, p.empty');
        var current = document.querySelectorAll('h2, table, p.empty');
        sections.forEach(function(el, i) {
          if (current[i]) current[i].replaceWith(el.cloneNode(true));
        });
        document.getElementById('updated').textContent = 'Updated ' + new Date().toLocaleTimeString();
      })
      .catch(function() {});
  }
  setInterval(refresh, interval);
})();
</script>
</body>
</html>"#;
