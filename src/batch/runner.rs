use std::sync::Arc;
use std::time::Duration;

use rapina::database::Db;
use rapina::prelude::*;
use rapina::sea_orm::DatabaseConnection;

use crate::config::app::AppConfig;
use crate::github::client::GitHubClient;
use crate::queue::service;

/// Spawn the batch runner as a background task.
/// Runs on its own interval, picking up queued PRs and merging them in batches.
pub fn spawn(
    conn: DatabaseConnection,
    github: Arc<GitHubClient>,
    config: AppConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(config.batch_interval_secs as u64);
        let mut ticker = tokio::time::interval(interval);
        let db = Db::new(conn);

        tracing::info!(
            interval_secs = config.batch_interval_secs,
            batch_size = config.batch_size,
            "Batch runner started"
        );

        loop {
            ticker.tick().await;

            if let Err(e) = run_once(&db, &github, &config).await {
                tracing::error!(error = %e, "Batch run failed");
            }
        }
    })
}

/// Execute a single batch cycle.
async fn run_once(
    db: &Db,
    github: &GitHubClient,
    config: &AppConfig,
) -> std::result::Result<(), BatchRunError> {
    let result = service::create_batch(db, config.batch_size)
        .await
        .map_err(|e| BatchRunError::Service(e.to_string()))?;

    let Some((batch, prs)) = result else {
        tracing::debug!("No PRs in queue, skipping batch");
        return Ok(());
    };

    tracing::info!(
        batch_id = batch.id,
        pr_count = prs.len(),
        "Batch created, checking CI"
    );

    service::update_batch_status(db, &batch, "testing")
        .await
        .map_err(|e| BatchRunError::Service(e.to_string()))?;

    // For each PR in the batch: check CI, then merge or fail
    let mut all_ok = true;

    for pr in &prs {
        // Get an installation token. We need the installation_id from the webhook,
        // but for now we'll get a fresh PR from the API to confirm state.
        // TODO: store installation_id in the PR record from the webhook payload
        let token = match github.get_installation_token(pr.installation_id).await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "Failed to get installation token");
                fail_batch(db, &batch, &prs, &format!("Auth failed: {e}")).await;
                return Ok(());
            }
        };

        // Set pending status on GitHub
        let _ = github
            .create_commit_status(
                &token,
                &pr.repo_owner,
                &pr.repo_name,
                &pr.head_sha,
                "pending",
                "Fila: checking CI status",
            )
            .await;

        // Check if all CI checks have passed
        match github
            .all_checks_passed(&token, &pr.repo_owner, &pr.repo_name, &pr.head_sha)
            .await
        {
            Ok(true) => {
                tracing::info!(pr = pr.pr_number, "CI passed");
                service::log_event(db, pr.id, batch.id, "ci_passed", None)
                    .await
                    .ok();
            }
            Ok(false) => {
                tracing::warn!(pr = pr.pr_number, "CI not passing, failing PR");
                service::mark_failed(db, pr, "CI checks not passing")
                    .await
                    .ok();
                let _ = github
                    .create_commit_status(
                        &token,
                        &pr.repo_owner,
                        &pr.repo_name,
                        &pr.head_sha,
                        "failure",
                        "Fila: CI checks not passing",
                    )
                    .await;
                all_ok = false;
                continue;
            }
            Err(e) => {
                tracing::error!(pr = pr.pr_number, error = %e, "Failed to check CI");
                service::mark_failed(db, pr, &format!("CI check failed: {e}"))
                    .await
                    .ok();
                all_ok = false;
                continue;
            }
        }
    }

    // Merge phase: only merge PRs that passed CI
    service::update_batch_status(db, &batch, "merging")
        .await
        .map_err(|e| BatchRunError::Service(e.to_string()))?;

    for pr in &prs {
        if pr.status == "failed" {
            continue;
        }

        let token = match github.get_installation_token(pr.installation_id).await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(pr = pr.pr_number, error = %e, "Token refresh failed during merge");
                service::mark_failed(db, pr, &format!("Auth failed: {e}"))
                    .await
                    .ok();
                all_ok = false;
                continue;
            }
        };

        match github
            .merge_pr(
                &token,
                &pr.repo_owner,
                &pr.repo_name,
                pr.pr_number,
                &pr.head_sha,
            )
            .await
        {
            Ok(resp) => {
                tracing::info!(pr = pr.pr_number, sha = resp.sha, "PR merged");
                service::mark_merged(db, pr).await.ok();
                service::log_event(db, pr.id, batch.id, "merged", Some(&resp.sha))
                    .await
                    .ok();

                let _ = github
                    .create_commit_status(
                        &token,
                        &pr.repo_owner,
                        &pr.repo_name,
                        &pr.head_sha,
                        "success",
                        "Fila: merged",
                    )
                    .await;
            }
            Err(e) => {
                tracing::error!(pr = pr.pr_number, error = %e, "Merge failed");
                service::mark_failed(db, pr, &format!("Merge failed: {e}"))
                    .await
                    .ok();
                let _ = github
                    .create_commit_status(
                        &token,
                        &pr.repo_owner,
                        &pr.repo_name,
                        &pr.head_sha,
                        "failure",
                        &format!("Fila: merge failed - {e}"),
                    )
                    .await;
                all_ok = false;
            }
        }
    }

    // Finalize batch
    let final_status = if all_ok { "done" } else { "failed" };
    service::update_batch_status(db, &batch, final_status)
        .await
        .map_err(|e| BatchRunError::Service(e.to_string()))?;

    tracing::info!(
        batch_id = batch.id,
        status = final_status,
        "Batch completed"
    );

    Ok(())
}

/// Mark all PRs in a batch as failed and close the batch.
async fn fail_batch(
    db: &Db,
    batch: &crate::entity::batch::Model,
    prs: &[crate::entity::pull_request::Model],
    reason: &str,
) {
    for pr in prs {
        service::mark_failed(db, pr, reason).await.ok();
    }
    service::update_batch_status(db, batch, "failed").await.ok();
}

#[derive(Debug)]
enum BatchRunError {
    Service(String),
}

impl std::fmt::Display for BatchRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Service(msg) => write!(f, "service error: {msg}"),
        }
    }
}
