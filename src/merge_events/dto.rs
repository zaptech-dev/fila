use rapina::schemars::{self, JsonSchema};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct CreateMergeEvent {
    pub pull_request_id: i32,
    pub batch_id: i32,
    pub event_type: String,
    pub details: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateMergeEvent {
    pub pull_request_id: Option<i32>,
    pub batch_id: Option<i32>,
    pub event_type: Option<String>,
    pub details: Option<String>,
}
