use std::sync::Arc;
use std::time::Duration;

use rapina::database::Db;
use rapina::prelude::*;
use rapina::sea_orm::DatabaseConnection;

use crate::config::app::AppConfig;
use crate::github::client::GitHubClient;
use crate::github::types::MergeResult;
use crate::queue::service;

const MERGE_BRANCH: &str = "fila/merge";

/// Spawn the merge queue runner as a background task.
/// Processes one PR at a time using the bors-style flow:
/// reset fila/merge to main, merge PR, wait for CI, fast-forward main.
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
            ci_timeout_secs = config.ci_timeout_secs,
            "Merge queue runner started"
        );

        loop {
            ticker.tick().await;

            if let Err(e) = run_once(&db, &github, &config).await {
                tracing::error!(error = %e, "Merge queue run failed");
            }
        }
    })
}

/// Execute a single merge cycle: pick next PR, test it, merge it.
async fn run_once(
    db: &Db,
    github: &GitHubClient,
    config: &AppConfig,
) -> std::result::Result<(), RunError> {
    let pr = match service::get_next_queued(db)
        .await
        .map_err(|e| RunError(e.to_string()))?
    {
        Some(pr) => pr,
        None => return Ok(()),
    };

    tracing::info!(
        pr = pr.pr_number,
        repo = format!("{}/{}", pr.repo_owner, pr.repo_name),
        "Processing PR"
    );

    service::mark_testing(db, &pr)
        .await
        .map_err(|e| RunError(e.to_string()))?;
    service::log_event(db, pr.id, 0, "testing", None).await.ok();

    let token = match github.get_installation_token(pr.installation_id).await {
        Ok(t) => t,
        Err(e) => {
            service::mark_failed(db, &pr, &format!("Auth failed: {e}"))
                .await
                .ok();
            return Err(RunError(e.to_string()));
        }
    };

    // Step 1: Get main HEAD
    let main_sha = match github
        .get_ref(&token, &pr.repo_owner, &pr.repo_name, "heads/main")
        .await
    {
        Ok(sha) => sha,
        Err(e) => {
            fail_pr_with_comment(
                db,
                github,
                &token,
                &pr,
                &format!("Failed to get main HEAD: {e}"),
            )
            .await;
            return Ok(());
        }
    };

    // Step 2: Reset fila/merge to main HEAD (create if it doesn't exist)
    let merge_ref = format!("heads/{MERGE_BRANCH}");
    if let Err(e) = github
        .ensure_ref(&token, &pr.repo_owner, &pr.repo_name, &merge_ref, &main_sha)
        .await
    {
        fail_pr_with_comment(
            db,
            github,
            &token,
            &pr,
            &format!("Failed to reset {MERGE_BRANCH}: {e}"),
        )
        .await;
        return Ok(());
    }

    // Step 3: Merge PR branch into fila/merge
    let merge_sha = match github
        .create_merge(
            &token,
            &pr.repo_owner,
            &pr.repo_name,
            MERGE_BRANCH,
            &pr.head_sha,
            &format!("Fila: testing #{} — {}", pr.pr_number, pr.title),
        )
        .await
    {
        Ok(MergeResult::Created(sha)) => {
            tracing::info!(pr = pr.pr_number, merge_sha = sha, "Merge commit created");
            service::log_event(db, pr.id, 0, "merge_created", Some(&sha))
                .await
                .ok();
            sha
        }
        Ok(MergeResult::AlreadyMerged) => {
            tracing::info!(pr = pr.pr_number, "PR already merged into main");
            service::mark_merged(db, &pr)
                .await
                .map_err(|e| RunError(e.to_string()))?;
            comment(
                github,
                &token,
                &pr,
                &format!("#{} is already merged into main.", pr.pr_number),
            )
            .await;
            return Ok(());
        }
        Ok(MergeResult::Conflict) => {
            fail_pr_with_comment(
                db,
                github,
                &token,
                &pr,
                &format!(
                    "Merge conflict: #{} cannot be cleanly merged into main. Rebase and `@fila ship` again.",
                    pr.pr_number
                ),
            )
            .await;
            return Ok(());
        }
        Err(e) => {
            fail_pr_with_comment(
                db,
                github,
                &token,
                &pr,
                &format!("Failed to create merge commit: {e}"),
            )
            .await;
            return Ok(());
        }
    };

    // Step 4: Wait for CI on the merge commit
    let poll_interval = Duration::from_secs(config.poll_interval_secs as u64);
    let timeout = Duration::from_secs(config.ci_timeout_secs as u64);
    let ci_result = poll_checks(github, &token, &pr, &merge_sha, poll_interval, timeout).await;

    match ci_result {
        CiResult::Passed => {
            tracing::info!(pr = pr.pr_number, "CI passed on merge commit");
            service::log_event(db, pr.id, 0, "ci_passed", Some(&merge_sha))
                .await
                .ok();
        }
        CiResult::Failed(details) => {
            fail_pr_with_comment(
                db,
                github,
                &token,
                &pr,
                &format!(
                    "CI failed on merge commit `{}`:\n{}",
                    &merge_sha[..8],
                    details
                ),
            )
            .await;
            return Ok(());
        }
        CiResult::Timeout => {
            fail_pr_with_comment(
                db,
                github,
                &token,
                &pr,
                &format!(
                    "CI timed out after {} minutes. `@fila ship` to retry.",
                    config.ci_timeout_secs / 60
                ),
            )
            .await;
            return Ok(());
        }
    }

    // Step 5: Fast-forward main to the merge commit
    match github
        .update_ref(
            &token,
            &pr.repo_owner,
            &pr.repo_name,
            "heads/main",
            &merge_sha,
            false,
        )
        .await
    {
        Ok(()) => {
            tracing::info!(
                pr = pr.pr_number,
                sha = merge_sha,
                "Main fast-forwarded, PR merged"
            );
            service::mark_merged(db, &pr)
                .await
                .map_err(|e| RunError(e.to_string()))?;
            service::log_event(db, pr.id, 0, "merged", Some(&merge_sha))
                .await
                .ok();
            comment(
                github,
                &token,
                &pr,
                &format!("#{} merged into main ({})", pr.pr_number, &merge_sha[..8]),
            )
            .await;
        }
        Err(e) => {
            // Fast-forward failed — someone pushed to main while CI was running.
            // Re-queue so it gets retried against the new main.
            tracing::warn!(
                pr = pr.pr_number,
                error = %e,
                "Fast-forward failed, re-queuing"
            );
            requeue_pr(db, &pr).await;
            service::log_event(
                db,
                pr.id,
                0,
                "requeued",
                Some("Main was updated during CI, retrying"),
            )
            .await
            .ok();
            comment(
                github,
                &token,
                &pr,
                "Main was updated while CI was running. Re-queued — will retry automatically.",
            )
            .await;
        }
    }

    Ok(())
}

enum CiResult {
    Passed,
    Failed(String),
    Timeout,
}

/// Poll check runs until all complete or timeout.
async fn poll_checks(
    github: &GitHubClient,
    token: &str,
    pr: &crate::entity::pull_request::Model,
    sha: &str,
    interval: Duration,
    timeout: Duration,
) -> CiResult {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        if tokio::time::Instant::now() >= deadline {
            return CiResult::Timeout;
        }

        tokio::time::sleep(interval).await;

        let checks = match github
            .get_check_runs(token, &pr.repo_owner, &pr.repo_name, sha)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch check runs, retrying");
                continue;
            }
        };

        if checks.check_runs.is_empty() {
            tracing::debug!(pr = pr.pr_number, "No check runs yet, waiting");
            continue;
        }

        let all_completed = checks.check_runs.iter().all(|c| c.status == "completed");
        if !all_completed {
            let pending: Vec<&str> = checks
                .check_runs
                .iter()
                .filter(|c| c.status != "completed")
                .map(|c| c.name.as_str())
                .collect();
            tracing::debug!(
                pr = pr.pr_number,
                pending = ?pending,
                "Waiting for checks"
            );
            continue;
        }

        // All completed — check conclusions
        let failed: Vec<String> = checks
            .check_runs
            .iter()
            .filter(|c| {
                !matches!(
                    c.conclusion.as_deref(),
                    Some("success") | Some("neutral") | Some("skipped")
                )
            })
            .map(|c| {
                format!(
                    "- **{}**: {}",
                    c.name,
                    c.conclusion.as_deref().unwrap_or("unknown")
                )
            })
            .collect();

        if failed.is_empty() {
            return CiResult::Passed;
        } else {
            return CiResult::Failed(failed.join("\n"));
        }
    }
}

async fn fail_pr_with_comment(
    db: &Db,
    github: &GitHubClient,
    token: &str,
    pr: &crate::entity::pull_request::Model,
    message: &str,
) {
    service::mark_failed(db, pr, message).await.ok();
    comment(github, token, pr, message).await;
}

async fn requeue_pr(db: &Db, pr: &crate::entity::pull_request::Model) {
    use rapina::sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
    let mut active = pr.clone().into_active_model();
    active.status = Set("queued".to_string());
    active.update(db.conn()).await.ok();
}

async fn comment(
    github: &GitHubClient,
    token: &str,
    pr: &crate::entity::pull_request::Model,
    body: &str,
) {
    if let Err(e) = github
        .create_issue_comment(token, &pr.repo_owner, &pr.repo_name, pr.pr_number, body)
        .await
    {
        tracing::warn!(pr = pr.pr_number, error = %e, "Failed to post comment");
    }
}

#[derive(Debug)]
struct RunError(String);

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
