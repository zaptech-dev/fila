use rapina::prelude::*;
use rapina::database::{Db, DbError};
use rapina::sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};

use crate::entity::MergeEvent;
use crate::entity::merge_event::{ActiveModel, Model};

use super::dto::{CreateMergeEvent, UpdateMergeEvent};
use super::error::MergeEventError;

#[get("/merge_events")]
#[errors(MergeEventError)]
pub async fn list_merge_events(db: Db) -> Result<Json<Vec<Model>>> {
    let items = MergeEvent::find().all(db.conn()).await.map_err(DbError)?;
    Ok(Json(items))
}

#[get("/merge_events/:id")]
#[errors(MergeEventError)]
pub async fn get_merge_event(db: Db, id: Path<i32>) -> Result<Json<Model>> {
    let id = id.into_inner();
    let item = MergeEvent::find_by_id(id)
        .one(db.conn())
        .await
        .map_err(DbError)?
        .ok_or_else(|| Error::not_found(format!("MergeEvent {} not found", id)))?;
    Ok(Json(item))
}

#[post("/merge_events")]
#[errors(MergeEventError)]
pub async fn create_merge_event(db: Db, body: Json<CreateMergeEvent>) -> Result<Json<Model>> {
    let input = body.into_inner();
    let item = ActiveModel {
        pull_request_id: Set(input.pull_request_id),
        batch_id: Set(input.batch_id),
        event_type: Set(input.event_type),
        details: Set(input.details),
        ..Default::default()
    };
    let result = item.insert(db.conn()).await.map_err(DbError)?;
    Ok(Json(result))
}

#[put("/merge_events/:id")]
#[errors(MergeEventError)]
pub async fn update_merge_event(db: Db, id: Path<i32>, body: Json<UpdateMergeEvent>) -> Result<Json<Model>> {
    let id = id.into_inner();
    let item = MergeEvent::find_by_id(id)
        .one(db.conn())
        .await
        .map_err(DbError)?
        .ok_or_else(|| Error::not_found(format!("MergeEvent {} not found", id)))?;

    let update = body.into_inner();
    let mut active: ActiveModel = item.into_active_model();
    if let Some(val) = update.pull_request_id {
        active.pull_request_id = Set(val);
    }
    if let Some(val) = update.batch_id {
        active.batch_id = Set(val);
    }
    if let Some(val) = update.event_type {
        active.event_type = Set(val);
    }
    if let Some(val) = update.details {
        active.details = Set(val);
    }

    let result = active.update(db.conn()).await.map_err(DbError)?;
    Ok(Json(result))
}

#[delete("/merge_events/:id")]
#[errors(MergeEventError)]
pub async fn delete_merge_event(db: Db, id: Path<i32>) -> Result<Json<serde_json::Value>> {
    let id = id.into_inner();
    let result = MergeEvent::delete_by_id(id)
        .exec(db.conn())
        .await
        .map_err(DbError)?;
    if result.rows_affected == 0 {
        return Err(Error::not_found(format!("MergeEvent {} not found", id)));
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}
