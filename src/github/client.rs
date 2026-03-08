use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::header::{ACCEPT, AUTHORIZATION};
use serde::Serialize;

use super::types::*;

const GITHUB_API: &str = "https://api.github.com";

#[derive(Debug, Serialize)]
struct JwtClaims {
    iat: u64,
    exp: u64,
    iss: String,
}

/// Cached installation token with expiry.
struct CachedToken {
    token: String,
    expires_at: u64, // unix timestamp
}

pub struct GitHubClient {
    client: reqwest::Client,
    app_id: String,
    private_key: String,
    /// Cached per-installation tokens. For simplicity, stores a single
    /// installation token (most Fila deployments target one installation).
    token_cache: RwLock<Option<CachedToken>>,
}

impl GitHubClient {
    pub fn new(app_id: String, private_key: String) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("fila-merge-queue/0.1")
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            app_id,
            private_key,
            token_cache: RwLock::new(None),
        }
    }

    /// Generate a short-lived JWT for GitHub App authentication.
    fn generate_jwt(&self) -> Result<String, GitHubClientError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = JwtClaims {
            iat: now - 60,        // clock skew tolerance
            exp: now + (10 * 60), // 10 minutes max
            iss: self.app_id.clone(),
        };

        let key = EncodingKey::from_rsa_pem(self.private_key.as_bytes())
            .map_err(|e| GitHubClientError::Auth(format!("Invalid private key: {e}")))?;

        encode(&Header::new(Algorithm::RS256), &claims, &key)
            .map_err(|e| GitHubClientError::Auth(format!("JWT encoding failed: {e}")))
    }

    /// Get an installation access token, using cache when possible.
    pub async fn get_installation_token(
        &self,
        installation_id: i64,
    ) -> Result<String, GitHubClientError> {
        // Check cache first
        {
            let cache = self.token_cache.read().unwrap();
            if let Some(ref cached) = *cache {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                // Refresh 5 minutes before expiry
                if cached.expires_at > now + 300 {
                    return Ok(cached.token.clone());
                }
            }
        }

        let jwt = self.generate_jwt()?;

        let resp = self
            .client
            .post(format!(
                "{GITHUB_API}/app/installations/{installation_id}/access_tokens"
            ))
            .header(AUTHORIZATION, format!("Bearer {jwt}"))
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(GitHubClientError::Api(format!(
                "Failed to get installation token: {status} {body}"
            )));
        }

        let token_resp: GhInstallationToken = resp
            .json()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))?;

        // Parse expiry and cache
        let expires_at = chrono::DateTime::parse_from_rfc3339(&token_resp.expires_at)
            .map(|dt| dt.timestamp() as u64)
            .unwrap_or(0);

        let token = token_resp.token.clone();

        {
            let mut cache = self.token_cache.write().unwrap();
            *cache = Some(CachedToken {
                token: token_resp.token,
                expires_at,
            });
        }

        Ok(token)
    }

    /// Make an authenticated GET request to the GitHub API.
    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
    ) -> Result<T, GitHubClientError> {
        let resp = self
            .client
            .get(format!("{GITHUB_API}{path}"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(GitHubClientError::Api(format!("{status} {body}")));
        }

        resp.json()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))
    }

    pub async fn get_pr(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        number: i32,
    ) -> Result<GhPullRequest, GitHubClientError> {
        self.get(token, &format!("/repos/{owner}/{repo}/pulls/{number}"))
            .await
    }

    pub async fn get_check_runs(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        sha: &str,
    ) -> Result<GhCheckRunsResponse, GitHubClientError> {
        self.get(
            token,
            &format!("/repos/{owner}/{repo}/commits/{sha}/check-runs"),
        )
        .await
    }

    pub async fn merge_pr(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        number: i32,
        sha: &str,
    ) -> Result<GhMergeResponse, GitHubClientError> {
        let body = GhMergeRequest {
            sha: sha.to_string(),
            merge_method: "squash".to_string(),
        };

        let resp = self
            .client
            .put(format!(
                "{GITHUB_API}/repos/{owner}/{repo}/pulls/{number}/merge"
            ))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, "application/vnd.github+json")
            .json(&body)
            .send()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubClientError::Api(format!(
                "Merge failed: {status} {text}"
            )));
        }

        resp.json()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))
    }

    pub async fn create_commit_status(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        sha: &str,
        state: &str,
        description: &str,
    ) -> Result<(), GitHubClientError> {
        let body = GhCommitStatus {
            state: state.to_string(),
            description: description.to_string(),
            context: "fila/merge-queue".to_string(),
        };

        let resp = self
            .client
            .post(format!("{GITHUB_API}/repos/{owner}/{repo}/statuses/{sha}"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, "application/vnd.github+json")
            .json(&body)
            .send()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubClientError::Api(format!(
                "Status update failed: {status} {text}"
            )));
        }

        Ok(())
    }

    /// Get a git reference SHA.
    pub async fn get_ref(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        git_ref: &str,
    ) -> Result<String, GitHubClientError> {
        let r: GhRef = self
            .get(token, &format!("/repos/{owner}/{repo}/git/ref/{git_ref}"))
            .await?;
        Ok(r.object.sha)
    }

    /// Create a new git reference.
    pub async fn create_ref(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        git_ref: &str,
        sha: &str,
    ) -> Result<(), GitHubClientError> {
        let body = serde_json::json!({
            "ref": format!("refs/{git_ref}"),
            "sha": sha,
        });

        let resp = self
            .client
            .post(format!("{GITHUB_API}/repos/{owner}/{repo}/git/refs"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, "application/vnd.github+json")
            .json(&body)
            .send()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubClientError::Api(format!(
                "Create ref failed: {status} {text}"
            )));
        }

        Ok(())
    }

    /// Update a git reference to a new SHA.
    pub async fn update_ref(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        git_ref: &str,
        sha: &str,
        force: bool,
    ) -> Result<(), GitHubClientError> {
        let body = serde_json::json!({
            "sha": sha,
            "force": force,
        });

        let resp = self
            .client
            .patch(format!(
                "{GITHUB_API}/repos/{owner}/{repo}/git/refs/{git_ref}"
            ))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, "application/vnd.github+json")
            .json(&body)
            .send()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubClientError::Api(format!(
                "Update ref failed: {status} {text}"
            )));
        }

        Ok(())
    }

    /// Ensure a git reference exists at the given SHA.
    /// Tries update first (force), falls back to create if the ref doesn't exist.
    pub async fn ensure_ref(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        git_ref: &str,
        sha: &str,
    ) -> Result<(), GitHubClientError> {
        match self
            .update_ref(token, owner, repo, git_ref, sha, true)
            .await
        {
            Ok(()) => Ok(()),
            Err(GitHubClientError::Api(msg)) if msg.contains("422") => {
                self.create_ref(token, owner, repo, git_ref, sha).await
            }
            Err(e) => Err(e),
        }
    }

    /// Create a merge commit via the GitHub API.
    pub async fn create_merge(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        base: &str,
        head: &str,
        message: &str,
    ) -> Result<MergeResult, GitHubClientError> {
        let body = serde_json::json!({
            "base": base,
            "head": head,
            "commit_message": message,
        });

        let resp = self
            .client
            .post(format!("{GITHUB_API}/repos/{owner}/{repo}/merges"))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, "application/vnd.github+json")
            .json(&body)
            .send()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        match status {
            201 => {
                let commit: GhMergeCommit = resp
                    .json()
                    .await
                    .map_err(|e| GitHubClientError::Http(e.to_string()))?;
                Ok(MergeResult::Created(commit.sha))
            }
            204 => Ok(MergeResult::AlreadyMerged),
            409 => Ok(MergeResult::Conflict),
            _ => {
                let text = resp.text().await.unwrap_or_default();
                Err(GitHubClientError::Api(format!(
                    "Merge failed: {status} {text}"
                )))
            }
        }
    }

    /// Post a comment on an issue/PR.
    pub async fn create_issue_comment(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        number: i32,
        body: &str,
    ) -> Result<(), GitHubClientError> {
        let payload = serde_json::json!({ "body": body });

        let resp = self
            .client
            .post(format!(
                "{GITHUB_API}/repos/{owner}/{repo}/issues/{number}/comments"
            ))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, "application/vnd.github+json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| GitHubClientError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(GitHubClientError::Api(format!(
                "Comment failed: {status} {text}"
            )));
        }

        Ok(())
    }

    /// Check if all check runs for a commit have passed.
    pub async fn all_checks_passed(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        sha: &str,
    ) -> Result<bool, GitHubClientError> {
        let checks = self.get_check_runs(token, owner, repo, sha).await?;

        if checks.check_runs.is_empty() {
            return Ok(false); // no checks = not ready
        }

        Ok(checks
            .check_runs
            .iter()
            .all(|c| c.status == "completed" && c.conclusion.as_deref() == Some("success")))
    }
}

#[derive(Debug)]
pub enum GitHubClientError {
    Auth(String),
    Http(String),
    Api(String),
}

impl std::fmt::Display for GitHubClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth(msg) => write!(f, "GitHub auth error: {msg}"),
            Self::Http(msg) => write!(f, "GitHub HTTP error: {msg}"),
            Self::Api(msg) => write!(f, "GitHub API error: {msg}"),
        }
    }
}

impl std::error::Error for GitHubClientError {}
