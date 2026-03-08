use rapina::database::{Db, DbError};
use rapina::prelude::*;
use rapina::sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, Set,
};

use crate::entity::PullRequest;
use crate::entity::batch::{ActiveModel as BatchActiveModel, Model as BatchModel};
use crate::entity::merge_event::ActiveModel as EventActiveModel;
use crate::entity::pull_request::{
    ActiveModel as PrActiveModel, Column as PrColumn, Model as PrModel,
};
use crate::github::types::GhPullRequest;

/// Find a PR currently in the queue.
pub async fn find_queued_pr(
    db: &Db,
    owner: &str,
    repo: &str,
    pr_number: i32,
) -> std::result::Result<Option<PrModel>, Error> {
    PullRequest::find()
        .filter(PrColumn::RepoOwner.eq(owner))
        .filter(PrColumn::RepoName.eq(repo))
        .filter(PrColumn::PrNumber.eq(pr_number))
        .filter(PrColumn::Status.eq("queued"))
        .one(db.conn())
        .await
        .map_err(|e| DbError(e).into_api_error())
}

/// Add a PR to the merge queue. No-op if already queued.
pub async fn enqueue(
    db: &Db,
    owner: &str,
    repo: &str,
    pr: &GhPullRequest,
    installation_id: i64,
) -> std::result::Result<(), Error> {
    if find_queued_pr(db, owner, repo, pr.number).await?.is_some() {
        return Ok(());
    }

    let active = PrActiveModel {
        repo_owner: Set(owner.to_string()),
        repo_name: Set(repo.to_string()),
        pr_number: Set(pr.number),
        title: Set(pr.title.clone()),
        author: Set(pr.user.login.clone()),
        head_sha: Set(pr.head.sha.clone()),
        status: Set("queued".to_string()),
        priority: Set(0),
        installation_id: Set(installation_id),
        queued_at: Set(Some(chrono::Utc::now())),
        merged_at: Set(None),
        ..Default::default()
    };

    active
        .insert(db.conn())
        .await
        .map_err(|e| DbError(e).into_api_error())?;

    Ok(())
}

/// Remove a PR from the queue by marking it as cancelled.
pub async fn dequeue(
    db: &Db,
    owner: &str,
    repo: &str,
    pr_number: i32,
) -> std::result::Result<(), Error> {
    let existing = find_queued_pr(db, owner, repo, pr_number).await?;

    if let Some(pr) = existing {
        let mut active = pr.into_active_model();
        active.status = Set("cancelled".to_string());
        active
            .update(db.conn())
            .await
            .map_err(|e| DbError(e).into_api_error())?;
    }

    Ok(())
}

/// Update the head SHA of a queued PR (after a force push).
pub async fn update_sha(
    db: &Db,
    owner: &str,
    repo: &str,
    pr_number: i32,
    sha: &str,
) -> std::result::Result<(), Error> {
    let existing = find_queued_pr(db, owner, repo, pr_number).await?;

    if let Some(pr) = existing {
        let mut active = pr.into_active_model();
        active.head_sha = Set(sha.to_string());
        active
            .update(db.conn())
            .await
            .map_err(|e| DbError(e).into_api_error())?;
    }

    Ok(())
}

/// Get all queued PRs ordered by priority (desc) then queued_at (asc).
pub async fn get_queue(db: &Db) -> std::result::Result<Vec<PrModel>, Error> {
    PullRequest::find()
        .filter(PrColumn::Status.eq("queued"))
        .order_by_desc(PrColumn::Priority)
        .order_by_asc(PrColumn::QueuedAt)
        .all(db.conn())
        .await
        .map_err(|e| DbError(e).into_api_error())
}

/// Take up to `size` queued PRs and group them into a new batch.
/// Returns None if the queue is empty.
pub async fn create_batch(
    db: &Db,
    size: usize,
) -> std::result::Result<Option<(BatchModel, Vec<PrModel>)>, Error> {
    let queued = get_queue(db).await?;
    if queued.is_empty() {
        return Ok(None);
    }

    let batch_prs: Vec<PrModel> = queued.into_iter().take(size).collect();

    // Create the batch record
    let batch = BatchActiveModel {
        status: Set("pending".to_string()),
        completed_at: Set(None),
        ..Default::default()
    };

    let batch = batch
        .insert(db.conn())
        .await
        .map_err(|e| DbError(e).into_api_error())?;

    // Mark each PR as batched and log the event
    for pr in &batch_prs {
        let mut active = pr.clone().into_active_model();
        active.status = Set("batched".to_string());
        active
            .update(db.conn())
            .await
            .map_err(|e| DbError(e).into_api_error())?;

        log_event(db, pr.id, batch.id, "batch_started", None).await?;
    }

    Ok(Some((batch, batch_prs)))
}

/// Mark a PR as merged.
pub async fn mark_merged(db: &Db, pr: &PrModel) -> std::result::Result<(), Error> {
    let mut active = pr.clone().into_active_model();
    active.status = Set("merged".to_string());
    active.merged_at = Set(Some(chrono::Utc::now()));
    active
        .update(db.conn())
        .await
        .map_err(|e| DbError(e).into_api_error())?;
    Ok(())
}

/// Mark a PR as failed.
pub async fn mark_failed(db: &Db, pr: &PrModel, reason: &str) -> std::result::Result<(), Error> {
    let mut active = pr.clone().into_active_model();
    active.status = Set("failed".to_string());
    active
        .update(db.conn())
        .await
        .map_err(|e| DbError(e).into_api_error())?;

    log_event(db, pr.id, 0, "failed", Some(reason)).await?;
    Ok(())
}

/// Update a batch's status.
pub async fn update_batch_status(
    db: &Db,
    batch: &BatchModel,
    status: &str,
) -> std::result::Result<(), Error> {
    let mut active = batch.clone().into_active_model();
    active.status = Set(status.to_string());
    if status == "done" || status == "failed" {
        active.completed_at = Set(Some(chrono::Utc::now()));
    }
    active
        .update(db.conn())
        .await
        .map_err(|e| DbError(e).into_api_error())?;
    Ok(())
}

/// Write an entry to the merge_events audit log.
pub async fn log_event(
    db: &Db,
    pull_request_id: i32,
    batch_id: i32,
    event_type: &str,
    details: Option<&str>,
) -> std::result::Result<(), Error> {
    let event = EventActiveModel {
        pull_request_id: Set(pull_request_id),
        batch_id: Set(batch_id),
        event_type: Set(event_type.to_string()),
        details: Set(details.map(|s| s.to_string())),
        ..Default::default()
    };

    event
        .insert(db.conn())
        .await
        .map_err(|e| DbError(e).into_api_error())?;

    Ok(())
}
