use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct GhPullRequest {
    pub number: i32,
    pub title: String,
    pub head: GhHead,
    pub user: GhUser,
    pub state: String,
    pub mergeable: Option<bool>,
    pub mergeable_state: Option<String>,
    pub labels: Vec<GhLabel>,
}

#[derive(Debug, Deserialize)]
pub struct GhHead {
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
}

#[derive(Debug, Deserialize)]
pub struct GhUser {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub struct GhLabel {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct GhCheckRunsResponse {
    pub total_count: i32,
    pub check_runs: Vec<GhCheckRun>,
}

#[derive(Debug, Deserialize)]
pub struct GhCheckRun {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GhMergeResponse {
    pub sha: String,
    pub merged: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct GhMergeRequest {
    pub sha: String,
    pub merge_method: String,
}

#[derive(Debug, Deserialize)]
pub struct GhInstallationToken {
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
pub struct GhRef {
    pub object: GhRefObject,
}

#[derive(Debug, Deserialize)]
pub struct GhRefObject {
    pub sha: String,
}

#[derive(Debug, Deserialize)]
pub struct GhMergeCommit {
    pub sha: String,
}

#[derive(Debug, Deserialize)]
pub struct GhAppInfo {
    pub name: String,
    pub slug: String,
}

pub enum MergeResult {
    Created(String),
    AlreadyMerged,
    Conflict,
}

#[derive(Debug, Serialize)]
pub struct GhCommitStatus {
    pub state: String,
    pub description: String,
    pub context: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookPayload {
    pub action: String,
    pub installation: Option<WebhookInstallation>,
    pub repository: Option<WebhookRepository>,
    pub pull_request: Option<GhPullRequest>,
    pub review: Option<WebhookReview>,
    pub check_suite: Option<WebhookCheckSuite>,
    pub label: Option<GhLabel>,
    pub comment: Option<WebhookComment>,
    pub issue: Option<WebhookIssue>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookComment {
    pub body: String,
    pub user: GhUser,
}

#[derive(Debug, Deserialize)]
pub struct WebhookIssue {
    pub number: i32,
    pub pull_request: Option<WebhookIssuePr>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookIssuePr {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookInstallation {
    pub id: i64,
}

#[derive(Debug, Deserialize)]
pub struct WebhookRepository {
    pub full_name: String,
    pub owner: GhUser,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookReview {
    pub state: String,
    pub user: GhUser,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookCheckSuite {
    pub conclusion: Option<String>,
    pub head_sha: String,
    pub pull_requests: Vec<WebhookCheckSuitePr>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookCheckSuitePr {
    pub number: i32,
}
