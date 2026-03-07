use std::sync::Arc;

use rapina::database::{Db, DbError};
use rapina::prelude::*;
use rapina::schemars;

use crate::config::app::AppConfig;
use crate::queue::service;

use super::client::GitHubClient;
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

// TODO: Add HMAC-SHA256 signature verification when Rapina gets a raw body extractor.
#[public]
#[post("/webhooks/github")]
#[errors(WebhookError)]
pub async fn handle_webhook(
    headers: Headers,
    body: Json<serde_json::Value>,
    _config: State<AppConfig>,
    _github: State<Arc<GitHubClient>>,
    db: Db,
) -> Result<Json<WebhookResponse>> {
    let event = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let payload: WebhookPayload = serde_json::from_value(body.into_inner())
        .map_err(|e| WebhookError::InvalidPayload(e.to_string()))?;

    tracing::info!(event = event, action = payload.action, "Webhook received");

    match event {
        "pull_request" => handle_pr_event(&payload, &db).await?,
        "pull_request_review" => handle_review_event(&payload, &db).await?,
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
    let pr = payload
        .pull_request
        .as_ref()
        .ok_or_else(|| WebhookError::InvalidPayload("Missing pull_request".into()).into_api_error())?;
    let repo = payload
        .repository
        .as_ref()
        .ok_or_else(|| WebhookError::InvalidPayload("Missing repository".into()).into_api_error())?;

    match payload.action.as_str() {
        "labeled" => {
            let label = payload.label.as_ref().map(|l| l.name.as_str());
            if label == Some("merge") {
                service::enqueue(db, &repo.owner.login, &repo.name, pr).await?;
                tracing::info!(pr = pr.number, "PR added to merge queue");
            }
        }
        "unlabeled" => {
            let label = payload.label.as_ref().map(|l| l.name.as_str());
            if label == Some("merge") {
                service::dequeue(db, &repo.owner.login, &repo.name, pr.number).await?;
                tracing::info!(pr = pr.number, "PR removed from merge queue");
            }
        }
        "closed" => {
            service::dequeue(db, &repo.owner.login, &repo.name, pr.number).await?;
            tracing::info!(pr = pr.number, "PR closed, removed from queue");
        }
        "synchronize" => {
            service::update_sha(db, &repo.owner.login, &repo.name, pr.number, &pr.head.sha)
                .await?;
            tracing::info!(pr = pr.number, sha = pr.head.sha, "PR head updated");
        }
        _ => {}
    }

    Ok(())
}

async fn handle_review_event(payload: &WebhookPayload, db: &Db) -> std::result::Result<(), Error> {
    let review = payload
        .review
        .as_ref()
        .ok_or_else(|| WebhookError::InvalidPayload("Missing review".into()).into_api_error())?;

    if review.state != "approved" {
        return Ok(());
    }

    let pr = payload
        .pull_request
        .as_ref()
        .ok_or_else(|| WebhookError::InvalidPayload("Missing pull_request".into()).into_api_error())?;
    let repo = payload
        .repository
        .as_ref()
        .ok_or_else(|| WebhookError::InvalidPayload("Missing repository".into()).into_api_error())?;

    let existing = service::find_queued_pr(db, &repo.owner.login, &repo.name, pr.number).await?;
    if existing.is_none() {
        if pr.labels.iter().any(|l| l.name == "merge") {
            service::enqueue(db, &repo.owner.login, &repo.name, pr).await?;
            tracing::info!(pr = pr.number, "PR auto-enqueued after approval");
        }
    }

    Ok(())
}

async fn handle_check_suite_event(payload: &WebhookPayload) -> std::result::Result<(), Error> {
    let suite = payload
        .check_suite
        .as_ref()
        .ok_or_else(|| WebhookError::InvalidPayload("Missing check_suite".into()).into_api_error())?;

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
