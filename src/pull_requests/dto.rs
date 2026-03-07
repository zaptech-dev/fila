use rapina::schemars::{self, JsonSchema};
use rapina::sea_orm::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct CreatePullRequest {
    pub repo_owner: String,
    pub repo_name: String,
    pub pr_number: i32,
    pub title: String,
    pub author: String,
    pub head_sha: String,
    pub status: String,
    pub priority: i32,
    pub merged_at: Option<DateTimeUtc>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdatePullRequest {
    pub repo_owner: Option<String>,
    pub repo_name: Option<String>,
    pub pr_number: Option<i32>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub head_sha: Option<String>,
    pub status: Option<String>,
    pub priority: Option<i32>,
    pub merged_at: Option<DateTimeUtc>,
}
