use std::sync::Arc;
use std::time::Duration;

use rapina::database::Db;
use rapina::prelude::*;
use rapina::sea_orm::DatabaseConnection;

use crate::config::app::AppConfig;
use crate::entity::pull_request::Model as PrModel;
use crate::github::client::GitHubClient;
use crate::github::types::MergeResult;
use crate::queue::service;

const MERGE_BRANCH: &str = "fila/merge";

/// Spawn the merge queue runner as a background task.
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
            strategy = config.merge_strategy,
            "Merge queue runner started"
        );

        match service::recover_orphaned(&db).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(count = n, "Recovered orphaned PRs back to queued"),
            Err(e) => tracing::error!(error = %e, "Failed to recover orphaned PRs"),
        }

        loop {
            ticker.tick().await;

            let result = match config.merge_strategy.as_str() {
                "sequential" => run_sequential(&db, &github, &config).await,
                _ => run_batch(&db, &github, &config).await,
            };

            if let Err(e) = result {
                tracing::error!(error = %e, "Merge queue run failed");
            }
        }
    })
}

/// Batch strategy: merge all queued PRs into fila/merge, run CI once, fast-forward once.
async fn run_batch(
    db: &Db,
    github: &GitHubClient,
    config: &AppConfig,
) -> std::result::Result<(), RunError> {
    let prs = service::get_queue(db)
        .await
        .map_err(|e| RunError(e.to_string()))?;

    if prs.is_empty() {
        return Ok(());
    }

    // Group by repo — process one repo per cycle
    let owner = prs[0].repo_owner.clone();
    let repo = prs[0].repo_name.clone();
    let installation_id = prs[0].installation_id;

    let batch_prs: Vec<PrModel> = prs
        .into_iter()
        .filter(|p| p.repo_owner == owner && p.repo_name == repo)
        .take(config.batch_size)
        .collect();

    tracing::info!(
        count = batch_prs.len(),
        repo = format!("{owner}/{repo}"),
        "Processing batch"
    );

    let token = match github.get_installation_token(installation_id).await {
        Ok(t) => t,
        Err(e) => {
            for pr in &batch_prs {
                service::mark_failed(db, pr, &format!("Auth failed: {e}"))
                    .await
                    .ok();
            }
            return Err(RunError(e.to_string()));
        }
    };

    // Step 1: Get main HEAD
    let base_ref = format!("heads/{}", config.base_branch);
    let main_sha = match github.get_ref(&token, &owner, &repo, &base_ref).await {
        Ok(sha) => sha,
        Err(e) => {
            for pr in &batch_prs {
                fail_pr_with_comment(
                    db,
                    github,
                    &token,
                    pr,
                    &format!("Failed to get {} HEAD: {e}", config.base_branch),
                )
                .await;
            }
            return Ok(());
        }
    };

    // Step 2: Reset fila/merge to base branch HEAD
    let merge_ref = format!("heads/{MERGE_BRANCH}");
    if let Err(e) = github
        .ensure_ref(&token, &owner, &repo, &merge_ref, &main_sha)
        .await
    {
        for pr in &batch_prs {
            fail_pr_with_comment(
                db,
                github,
                &token,
                pr,
                &format!("Failed to reset {MERGE_BRANCH}: {e}"),
            )
            .await;
        }
        return Ok(());
    }

    // Step 3: Merge each PR sequentially into fila/merge
    let mut merged_prs: Vec<&PrModel> = Vec::new();
    let mut final_sha = main_sha;

    for pr in &batch_prs {
        service::mark_testing(db, pr)
            .await
            .map_err(|e| RunError(e.to_string()))?;
        service::log_event(db, pr.id, 0, "testing", None).await.ok();

        match github
            .create_merge(
                &token,
                &owner,
                &repo,
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
                final_sha = sha;
                merged_prs.push(pr);
            }
            Ok(MergeResult::AlreadyMerged) => {
                tracing::info!(pr = pr.pr_number, "PR already merged");
                service::mark_merged(db, pr).await.ok();
                comment(
                    github,
                    &token,
                    pr,
                    &format!(
                        "#{} is already merged into {}.",
                        pr.pr_number, config.base_branch
                    ),
                )
                .await;
                close_pr(github, &token, pr).await;
            }
            Ok(MergeResult::Conflict) => {
                fail_pr_with_comment(
                    db,
                    github,
                    &token,
                    pr,
                    &format!(
                        "Merge conflict: #{} cannot be cleanly merged. Rebase and `@fila ship` again.",
                        pr.pr_number
                    ),
                )
                .await;
            }
            Err(e) => {
                fail_pr_with_comment(
                    db,
                    github,
                    &token,
                    pr,
                    &format!("Failed to create merge commit: {e}"),
                )
                .await;
            }
        }
    }

    if merged_prs.is_empty() {
        return Ok(());
    }

    let pr_numbers: Vec<i32> = merged_prs.iter().map(|p| p.pr_number).collect();
    tracing::info!(
        prs = ?pr_numbers,
        merge_sha = final_sha,
        "Batch merged, waiting for CI"
    );

    // Step 4: Wait for CI on the final merge SHA
    let poll_interval = Duration::from_secs(config.poll_interval_secs as u64);
    let timeout = Duration::from_secs(config.ci_timeout_secs as u64);
    let ci_result = poll_checks(
        github,
        &token,
        &owner,
        &repo,
        &final_sha,
        poll_interval,
        timeout,
    )
    .await;

    match ci_result {
        CiResult::Passed => {
            tracing::info!(prs = ?pr_numbers, "CI passed on batch merge commit");
            for pr in &merged_prs {
                service::log_event(db, pr.id, 0, "ci_passed", Some(&final_sha))
                    .await
                    .ok();
            }
        }
        CiResult::Failed(details) => {
            let msg = format!(
                "CI failed on batch merge commit `{}`:\n{}",
                &final_sha[..8],
                details
            );
            for pr in &merged_prs {
                fail_pr_with_comment(db, github, &token, pr, &msg).await;
            }
            return Ok(());
        }
        CiResult::Timeout => {
            let msg = format!(
                "CI timed out after {} minutes. `@fila ship` to retry.",
                config.ci_timeout_secs / 60
            );
            for pr in &merged_prs {
                fail_pr_with_comment(db, github, &token, pr, &msg).await;
            }
            return Ok(());
        }
    }

    // Step 5: Fast-forward base branch to the final merge commit
    match github
        .update_ref(&token, &owner, &repo, &base_ref, &final_sha, false)
        .await
    {
        Ok(()) => {
            tracing::info!(
                prs = ?pr_numbers,
                sha = final_sha,
                branch = config.base_branch,
                "Base branch fast-forwarded, batch merged"
            );
            for pr in &merged_prs {
                service::mark_merged(db, pr)
                    .await
                    .map_err(|e| RunError(e.to_string()))?;
                service::log_event(db, pr.id, 0, "merged", Some(&final_sha))
                    .await
                    .ok();
                comment(
                    github,
                    &token,
                    pr,
                    &format!(
                        "#{} merged into {} ({})",
                        pr.pr_number,
                        config.base_branch,
                        &final_sha[..8]
                    ),
                )
                .await;
                close_pr(github, &token, pr).await;
            }
        }
        Err(e) => {
            tracing::warn!(
                prs = ?pr_numbers,
                error = %e,
                "Fast-forward failed, re-queuing batch"
            );
            for pr in &merged_prs {
                requeue_pr(db, pr).await;
                service::log_event(
                    db,
                    pr.id,
                    0,
                    "requeued",
                    Some(&format!(
                        "{} was updated during CI, retrying",
                        config.base_branch
                    )),
                )
                .await
                .ok();
                comment(
                    github,
                    &token,
                    pr,
                    &format!("{} was updated while CI was running. Re-queued — will retry automatically.", config.base_branch),
                )
                .await;
            }
        }
    }

    Ok(())
}

/// Sequential strategy: process one PR at a time (original bors-style).
async fn run_sequential(
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

    let base_ref = format!("heads/{}", config.base_branch);
    let main_sha = match github
        .get_ref(&token, &pr.repo_owner, &pr.repo_name, &base_ref)
        .await
    {
        Ok(sha) => sha,
        Err(e) => {
            fail_pr_with_comment(
                db,
                github,
                &token,
                &pr,
                &format!("Failed to get {} HEAD: {e}", config.base_branch),
            )
            .await;
            return Ok(());
        }
    };

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
            tracing::info!(pr = pr.pr_number, "PR already merged");
            service::mark_merged(db, &pr)
                .await
                .map_err(|e| RunError(e.to_string()))?;
            comment(
                github,
                &token,
                &pr,
                &format!(
                    "#{} is already merged into {}.",
                    pr.pr_number, config.base_branch
                ),
            )
            .await;
            close_pr(github, &token, &pr).await;
            return Ok(());
        }
        Ok(MergeResult::Conflict) => {
            fail_pr_with_comment(
                db,
                github,
                &token,
                &pr,
                &format!(
                    "Merge conflict: #{} cannot be cleanly merged into {}. Rebase and `@fila ship` again.",
                    pr.pr_number, config.base_branch
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

    let poll_interval = Duration::from_secs(config.poll_interval_secs as u64);
    let timeout = Duration::from_secs(config.ci_timeout_secs as u64);
    let ci_result = poll_checks(
        github,
        &token,
        &pr.repo_owner,
        &pr.repo_name,
        &merge_sha,
        poll_interval,
        timeout,
    )
    .await;

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

    match github
        .update_ref(
            &token,
            &pr.repo_owner,
            &pr.repo_name,
            &base_ref,
            &merge_sha,
            false,
        )
        .await
    {
        Ok(()) => {
            tracing::info!(
                pr = pr.pr_number,
                sha = merge_sha,
                branch = config.base_branch,
                "Base branch fast-forwarded, PR merged"
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
                &format!(
                    "#{} merged into {} ({})",
                    pr.pr_number,
                    config.base_branch,
                    &merge_sha[..8]
                ),
            )
            .await;
            close_pr(github, &token, &pr).await;
        }
        Err(e) => {
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
                Some(&format!(
                    "{} was updated during CI, retrying",
                    config.base_branch
                )),
            )
            .await
            .ok();
            comment(
                github,
                &token,
                &pr,
                &format!(
                    "{} was updated while CI was running. Re-queued — will retry automatically.",
                    config.base_branch
                ),
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
    owner: &str,
    repo: &str,
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

        let checks = match github.get_check_runs(token, owner, repo, sha).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch check runs, retrying");
                continue;
            }
        };

        if checks.check_runs.is_empty() {
            tracing::debug!(sha = &sha[..8], "No check runs yet, waiting");
            continue;
        }

        // Deduplicate check runs by name, keeping the latest (highest ID).
        // GitHub can produce duplicate runs (e.g. from re-triggers) where
        // an older run stays "in_progress" forever while the newer one completed.
        let mut latest: std::collections::HashMap<&str, &crate::github::types::GhCheckRun> =
            std::collections::HashMap::new();
        for run in &checks.check_runs {
            let entry = latest.entry(run.name.as_str()).or_insert(run);
            if run.id > entry.id {
                *entry = run;
            }
        }
        let deduped: Vec<&&crate::github::types::GhCheckRun> = latest.values().collect();

        let all_completed = deduped.iter().all(|c| c.status == "completed");
        if !all_completed {
            let pending: Vec<&str> = deduped
                .iter()
                .filter(|c| c.status != "completed")
                .map(|c| c.name.as_str())
                .collect();
            tracing::debug!(
                sha = &sha[..8],
                pending = ?pending,
                "Waiting for checks"
            );
            continue;
        }

        let failed: Vec<String> = deduped
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
    pr: &PrModel,
    message: &str,
) {
    service::mark_failed(db, pr, message).await.ok();
    comment(github, token, pr, message).await;
}

async fn requeue_pr(db: &Db, pr: &PrModel) {
    use rapina::sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
    let mut active = pr.clone().into_active_model();
    active.status = Set(crate::types::PrStatus::Queued.to_string());
    active.update(db.conn()).await.ok();
}

async fn comment(github: &GitHubClient, token: &str, pr: &PrModel, body: &str) {
    if let Err(e) = github
        .create_issue_comment(token, &pr.repo_owner, &pr.repo_name, pr.pr_number, body)
        .await
    {
        tracing::warn!(pr = pr.pr_number, error = %e, "Failed to post comment");
    }
}

async fn close_pr(github: &GitHubClient, token: &str, pr: &PrModel) {
    if let Err(e) = github
        .close_pr(token, &pr.repo_owner, &pr.repo_name, pr.pr_number)
        .await
    {
        tracing::warn!(pr = pr.pr_number, error = %e, "Failed to close PR");
    }
}

#[derive(Debug)]
struct RunError(String);

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
