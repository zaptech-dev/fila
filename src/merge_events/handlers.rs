use rapina::database::{Db, DbError};
use rapina::prelude::*;
use rapina::sea_orm::EntityTrait;

use crate::entity::MergeEvent;
use crate::entity::merge_event::Model;

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
