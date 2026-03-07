use rapina::database::{Db, DbError};
use rapina::prelude::*;
use rapina::sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};

use crate::entity::Batch;
use crate::entity::batch::{ActiveModel, Model};

use super::dto::{CreateBatch, UpdateBatch};
use super::error::BatchError;

#[get("/batches")]
#[errors(BatchError)]
pub async fn list_batchs(db: Db) -> Result<Json<Vec<Model>>> {
    let items = Batch::find().all(db.conn()).await.map_err(DbError)?;
    Ok(Json(items))
}

#[get("/batches/:id")]
#[errors(BatchError)]
pub async fn get_batch(db: Db, id: Path<i32>) -> Result<Json<Model>> {
    let id = id.into_inner();
    let item = Batch::find_by_id(id)
        .one(db.conn())
        .await
        .map_err(DbError)?
        .ok_or_else(|| Error::not_found(format!("Batch {} not found", id)))?;
    Ok(Json(item))
}

#[post("/batches")]
#[errors(BatchError)]
pub async fn create_batch(db: Db, body: Json<CreateBatch>) -> Result<Json<Model>> {
    let input = body.into_inner();
    let item = ActiveModel {
        status: Set(input.status),
        completed_at: Set(input.completed_at),
        ..Default::default()
    };
    let result = item.insert(db.conn()).await.map_err(DbError)?;
    Ok(Json(result))
}

#[put("/batches/:id")]
#[errors(BatchError)]
pub async fn update_batch(db: Db, id: Path<i32>, body: Json<UpdateBatch>) -> Result<Json<Model>> {
    let id = id.into_inner();
    let item = Batch::find_by_id(id)
        .one(db.conn())
        .await
        .map_err(DbError)?
        .ok_or_else(|| Error::not_found(format!("Batch {} not found", id)))?;

    let update = body.into_inner();
    let mut active: ActiveModel = item.into_active_model();
    if let Some(val) = update.status {
        active.status = Set(val);
    }
    if let Some(val) = update.completed_at {
        active.completed_at = Set(val);
    }

    let result = active.update(db.conn()).await.map_err(DbError)?;
    Ok(Json(result))
}

#[delete("/batches/:id")]
#[errors(BatchError)]
pub async fn delete_batch(db: Db, id: Path<i32>) -> Result<Json<serde_json::Value>> {
    let id = id.into_inner();
    let result = Batch::delete_by_id(id)
        .exec(db.conn())
        .await
        .map_err(DbError)?;
    if result.rows_affected == 0 {
        return Err(Error::not_found(format!("Batch {} not found", id)));
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}
