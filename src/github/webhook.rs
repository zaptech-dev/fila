use std::sync::Arc;

use rapina::database::{Db, DbError};
use rapina::prelude::*;
use rapina::schemars;

use crate::config::app::AppConfig;
use crate::queue::service;

use super::client::GitHubClient;
use super::signature::SignedBody;
use super::types::WebhookPayload;

#[derive(Serialize, JsonSchema)]
pub struct WebhookResponse {
    pub status: String,
}

pub enum WebhookError {
    InvalidPayload(String),
    Db(DbError),
}

impl IntoApiError for WebhookError {
    fn into_api_error(self) -> Error {
        match self {
            Self::InvalidPayload(msg) => Error::bad_request(format!("Invalid payload: {msg}")),
            Self::Db(e) => e.into_api_error(),
        }
    }
}

impl DocumentedError for WebhookError {
    fn error_variants() -> Vec<ErrorVariant> {
        vec![
            ErrorVariant {
                status: 401,
                code: "UNAUTHORIZED",
                description: "Missing or invalid webhook signature",
            },
            ErrorVariant {
                status: 400,
                code: "BAD_REQUEST",
                description: "Invalid webhook payload",
            },
        ]
    }
}

impl From<DbError> for WebhookError {
    fn from(e: DbError) -> Self {
        Self::Db(e)
    }
}

#[public]
#[post("/webhooks/github")]
#[errors(WebhookError)]
pub async fn handle_webhook(
    headers: Headers,
    body: SignedBody<WebhookPayload>,
    config: State<AppConfig>,
    github: State<Arc<GitHubClient>>,
    db: Db,
) -> Result<Json<WebhookResponse>> {
    let event = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let payload = body.into_inner();

    tracing::info!(event = event, action = payload.action, "Webhook received");

    match event {
        "pull_request" => handle_pr_event(&payload, &db).await?,
        "pull_request_review" => handle_review_event(&payload, &db, &github, &config).await?,
        "issue_comment" => handle_comment_event(&payload, &db, &github, &config).await?,
        "check_suite" => handle_check_suite_event(&payload).await?,
        _ => {
            tracing::debug!(event = event, "Ignoring unhandled event type");
        }
    }

    Ok(Json(WebhookResponse {
        status: "ok".to_string(),
    }))
}

async fn handle_pr_event(payload: &WebhookPayload, db: &Db) -> std::result::Result<(), Error> {
    let pr = payload.pull_request.as_ref().ok_or_else(|| {
        WebhookError::InvalidPayload("Missing pull_request".into()).into_api_error()
    })?;
    let repo = payload.repository.as_ref().ok_or_else(|| {
        WebhookError::InvalidPayload("Missing repository".into()).into_api_error()
    })?;

    match payload.action.as_str() {
        "closed" => {
            service::dequeue(db, &repo.owner.login, &repo.name, pr.number).await?;
            tracing::info!(pr = pr.number, "PR closed, removed from queue");
        }
        "synchronize" => {
            service::update_sha(db, &repo.owner.login, &repo.name, pr.number, &pr.head.sha).await?;
            tracing::info!(pr = pr.number, sha = pr.head.sha, "PR head updated");
        }
        _ => {}
    }

    Ok(())
}

async fn handle_comment_event(
    payload: &WebhookPayload,
    db: &Db,
    github: &Arc<GitHubClient>,
    config: &AppConfig,
) -> std::result::Result<(), Error> {
    if payload.action != "created" {
        return Ok(());
    }

    let comment = payload
        .comment
        .as_ref()
        .ok_or_else(|| WebhookError::InvalidPayload("Missing comment".into()).into_api_error())?;
    let issue = payload
        .issue
        .as_ref()
        .ok_or_else(|| WebhookError::InvalidPayload("Missing issue".into()).into_api_error())?;
    let repo = payload.repository.as_ref().ok_or_else(|| {
        WebhookError::InvalidPayload("Missing repository".into()).into_api_error()
    })?;

    // Only handle comments on PRs (issues with a pull_request field)
    if issue.pull_request.is_none() {
        return Ok(());
    }

    let body = comment.body.trim().to_lowercase();
    let installation_id = payload.installation.as_ref().map(|i| i.id).unwrap_or(0);

    if body == "@fila ship" {
        let token = github
            .get_installation_token(installation_id)
            .await
            .map_err(|e| WebhookError::InvalidPayload(e.to_string()).into_api_error())?;

        let pr = github
            .get_pr(&token, &repo.owner.login, &repo.name, issue.number)
            .await
            .map_err(|e| WebhookError::InvalidPayload(e.to_string()).into_api_error())?;

        service::enqueue(db, &repo.owner.login, &repo.name, &pr, installation_id).await?;
        tracing::info!(
            pr = issue.number,
            user = comment.user.login,
            "PR added to merge queue via comment"
        );

        let short_sha = &pr.head.sha[..7.min(pr.head.sha.len())];
        let reply = queue_confirmation(short_sha, &comment.user.login, &config.dashboard_url);
        let _ = github
            .create_issue_comment(&token, &repo.owner.login, &repo.name, issue.number, &reply)
            .await;
    } else if body == "@fila cancel" {
        service::dequeue(db, &repo.owner.login, &repo.name, issue.number).await?;
        tracing::info!(
            pr = issue.number,
            user = comment.user.login,
            "PR removed from merge queue via comment"
        );

        if let Ok(token) = github.get_installation_token(installation_id).await {
            let reply = format!(
                "PR #{} has been removed from the merge queue by @{}.",
                issue.number, comment.user.login,
            );
            let _ = github
                .create_issue_comment(&token, &repo.owner.login, &repo.name, issue.number, &reply)
                .await;
        }
    } else if body == "@fila status" {
        let token = github
            .get_installation_token(installation_id)
            .await
            .map_err(|e| WebhookError::InvalidPayload(e.to_string()).into_api_error())?;

        let queue = service::get_queue(db).await?;
        let reply = if queue.is_empty() {
            "Queue is empty.".to_string()
        } else {
            let mut lines = vec![format!("**Queue ({} PRs):**", queue.len())];
            for (i, pr) in queue.iter().enumerate() {
                lines.push(format!(
                    "{}. #{} — {} (`{}`, {})",
                    i + 1,
                    pr.pr_number,
                    pr.title,
                    pr.status,
                    pr.repo_name
                ));
            }
            lines.join("\n")
        };

        let _ = github
            .create_issue_comment(&token, &repo.owner.login, &repo.name, issue.number, &reply)
            .await;
    }

    Ok(())
}

async fn handle_review_event(
    payload: &WebhookPayload,
    db: &Db,
    github: &Arc<GitHubClient>,
    config: &AppConfig,
) -> std::result::Result<(), Error> {
    let review = payload
        .review
        .as_ref()
        .ok_or_else(|| WebhookError::InvalidPayload("Missing review".into()).into_api_error())?;
    let pr = payload.pull_request.as_ref().ok_or_else(|| {
        WebhookError::InvalidPayload("Missing pull_request".into()).into_api_error()
    })?;
    let repo = payload.repository.as_ref().ok_or_else(|| {
        WebhookError::InvalidPayload("Missing repository".into()).into_api_error()
    })?;

    if review.state == "approved" {
        tracing::info!(pr = pr.number, reviewer = review.user.login, "PR approved");
    }

    // Check if the review body contains a fila command
    let body = review.body.as_deref().unwrap_or("").trim().to_lowercase();
    let installation_id = payload.installation.as_ref().map(|i| i.id).unwrap_or(0);

    if body == "@fila ship" || body.contains("@fila ship") {
        let token = github
            .get_installation_token(installation_id)
            .await
            .map_err(|e| WebhookError::InvalidPayload(e.to_string()).into_api_error())?;

        let gh_pr = github
            .get_pr(&token, &repo.owner.login, &repo.name, pr.number)
            .await
            .map_err(|e| WebhookError::InvalidPayload(e.to_string()).into_api_error())?;

        service::enqueue(db, &repo.owner.login, &repo.name, &gh_pr, installation_id).await?;
        tracing::info!(
            pr = pr.number,
            user = review.user.login,
            "PR added to merge queue via review"
        );

        let short_sha = &gh_pr.head.sha[..7.min(gh_pr.head.sha.len())];
        let reply = queue_confirmation(short_sha, &review.user.login, &config.dashboard_url);
        let _ = github
            .create_issue_comment(&token, &repo.owner.login, &repo.name, pr.number, &reply)
            .await;
    }

    Ok(())
}

async fn handle_check_suite_event(payload: &WebhookPayload) -> std::result::Result<(), Error> {
    let suite = payload.check_suite.as_ref().ok_or_else(|| {
        WebhookError::InvalidPayload("Missing check_suite".into()).into_api_error()
    })?;

    if payload.action != "completed" {
        return Ok(());
    }

    tracing::info!(
        sha = suite.head_sha,
        conclusion = ?suite.conclusion,
        prs = suite.pull_requests.len(),
        "Check suite completed"
    );

    Ok(())
}

fn queue_confirmation(short_sha: &str, user: &str, dashboard_url: &str) -> String {
    let mut msg = format!(
        "Commit {} has been added to the merge queue by @{}.",
        short_sha, user,
    );
    if !dashboard_url.is_empty() {
        msg.push_str(&format!("\n\n[View queue]({})", dashboard_url));
    }
    msg
}
