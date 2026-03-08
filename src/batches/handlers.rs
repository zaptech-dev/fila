use rapina::database::{Db, DbError};
use rapina::prelude::*;
use rapina::sea_orm::EntityTrait;

use crate::entity::Batch;
use crate::entity::batch::Model;

use super::error::BatchError;

#[get("/batches")]
#[errors(BatchError)]
pub async fn list_batches(db: Db) -> Result<Json<Vec<Model>>> {
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
