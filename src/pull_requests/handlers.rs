use rapina::database::{Db, DbError};
use rapina::prelude::*;
use rapina::sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};

use crate::entity::PullRequest;
use crate::entity::pull_request::{ActiveModel, Model};

use super::dto::{CreatePullRequest, UpdatePullRequest};
use super::error::PullRequestError;

#[get("/pull_requests")]
#[errors(PullRequestError)]
pub async fn list_pull_requests(db: Db) -> Result<Json<Vec<Model>>> {
    let items = PullRequest::find().all(db.conn()).await.map_err(DbError)?;
    Ok(Json(items))
}

#[get("/pull_requests/:id")]
#[errors(PullRequestError)]
pub async fn get_pull_request(db: Db, id: Path<i32>) -> Result<Json<Model>> {
    let id = id.into_inner();
    let item = PullRequest::find_by_id(id)
        .one(db.conn())
        .await
        .map_err(DbError)?
        .ok_or_else(|| Error::not_found(format!("PullRequest {} not found", id)))?;
    Ok(Json(item))
}

#[post("/pull_requests")]
#[errors(PullRequestError)]
pub async fn create_pull_request(db: Db, body: Json<CreatePullRequest>) -> Result<Json<Model>> {
    let input = body.into_inner();
    let item = ActiveModel {
        repo_owner: Set(input.repo_owner),
        repo_name: Set(input.repo_name),
        pr_number: Set(input.pr_number),
        title: Set(input.title),
        author: Set(input.author),
        head_sha: Set(input.head_sha),
        status: Set(input.status),
        priority: Set(input.priority),
        queued_at: Set(Some(chrono::Utc::now().naive_utc())),
        merged_at: Set(input.merged_at),
        ..Default::default()
    };
    let result = item.insert(db.conn()).await.map_err(DbError)?;
    Ok(Json(result))
}

#[put("/pull_requests/:id")]
#[errors(PullRequestError)]
pub async fn update_pull_request(
    db: Db,
    id: Path<i32>,
    body: Json<UpdatePullRequest>,
) -> Result<Json<Model>> {
    let id = id.into_inner();
    let item = PullRequest::find_by_id(id)
        .one(db.conn())
        .await
        .map_err(DbError)?
        .ok_or_else(|| Error::not_found(format!("PullRequest {} not found", id)))?;

    let update = body.into_inner();
    let mut active: ActiveModel = item.into_active_model();
    if let Some(val) = update.repo_owner {
        active.repo_owner = Set(val);
    }
    if let Some(val) = update.repo_name {
        active.repo_name = Set(val);
    }
    if let Some(val) = update.pr_number {
        active.pr_number = Set(val);
    }
    if let Some(val) = update.title {
        active.title = Set(val);
    }
    if let Some(val) = update.author {
        active.author = Set(val);
    }
    if let Some(val) = update.head_sha {
        active.head_sha = Set(val);
    }
    if let Some(val) = update.status {
        active.status = Set(val);
    }
    if let Some(val) = update.priority {
        active.priority = Set(val);
    }
    if let Some(val) = update.merged_at {
        active.merged_at = Set(val);
    }

    let result = active.update(db.conn()).await.map_err(DbError)?;
    Ok(Json(result))
}

#[delete("/pull_requests/:id")]
#[errors(PullRequestError)]
pub async fn delete_pull_request(db: Db, id: Path<i32>) -> Result<Json<serde_json::Value>> {
    let id = id.into_inner();
    let result = PullRequest::delete_by_id(id)
        .exec(db.conn())
        .await
        .map_err(DbError)?;
    if result.rows_affected == 0 {
        return Err(Error::not_found(format!("PullRequest {} not found", id)));
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}
