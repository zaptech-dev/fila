use rapina::schemars::{self, JsonSchema};
use rapina::sea_orm::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct CreateBatch {
    pub status: String,
    pub completed_at: Option<DateTimeUtc>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateBatch {
    pub status: Option<String>,
    pub completed_at: Option<DateTimeUtc>,
}
