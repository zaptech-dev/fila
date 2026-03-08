use rapina::database::{Db, DbError};
use rapina::prelude::*;
use rapina::sea_orm::EntityTrait;

use crate::entity::PullRequest;
use crate::entity::pull_request::Model;
use crate::errors::CrudError;

#[get("/pull_requests")]
#[errors(CrudError)]
pub async fn list_pull_requests(db: Db) -> Result<Json<Vec<Model>>> {
    let items = PullRequest::find().all(db.conn()).await.map_err(DbError)?;
    Ok(Json(items))
}

#[get("/pull_requests/:id")]
#[errors(CrudError)]
pub async fn get_pull_request(db: Db, id: Path<i32>) -> Result<Json<Model>> {
    let id = id.into_inner();
    let item = PullRequest::find_by_id(id)
        .one(db.conn())
        .await
        .map_err(DbError)?
        .ok_or_else(|| Error::not_found(format!("PullRequest {} not found", id)))?;
    Ok(Json(item))
}
