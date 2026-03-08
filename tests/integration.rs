use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use hmac::{Hmac, Mac};
use rapina::prelude::*;
use rapina::testing::TestClient;
use sha2::Sha256;

use fila::config::app::AppConfig;
use fila::github::client::GitHubClient;

const TEST_SECRET: &str = "test-secret";

static TEST_DB_COUNTER: AtomicU32 = AtomicU32::new(0);

fn next_db_path() -> String {
    let id = TEST_DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = format!("/tmp/fila_test_{}.db", id);
    let _ = std::fs::remove_file(&path);
    path
}

async fn build_test_app(db_path: &str) -> TestClient {
    let config = AppConfig {
        database_url: format!("sqlite://{}?mode=rwc", db_path),
        server_port: 0,
        host: "127.0.0.1".to_string(),
        github_app_id: "test-app-id".to_string(),
        github_private_key: "test-private-key".to_string(),
        github_webhook_secret: TEST_SECRET.to_string(),
        merge_strategy: "batch".to_string(),
        batch_size: 5,
        batch_interval_secs: 300,
        ci_timeout_secs: 1800,
        poll_interval_secs: 15,
        dashboard_url: "".to_string(),
        base_branch: "main".to_string(),
    };

    let github = Arc::new(GitHubClient::new(
        config.github_app_id.clone(),
        config.github_private_key.clone(),
    ));
    let app = fila::build_app(config, github, false).await;
    TestClient::new(app).await
}

fn sign_payload(payload: &serde_json::Value) -> String {
    let body = serde_json::to_vec(payload).unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(TEST_SECRET.as_bytes()).unwrap();
    mac.update(&body);
    let result = mac.finalize().into_bytes();
    format!("sha256={}", hex::encode(result))
}

/// Insert a queued PR directly into the DB for test setup.
fn insert_test_pr(db_path: &str, pr_number: i32, head_sha: &str) {
    let sql = format!(
        "INSERT INTO pull_requests (repo_owner, repo_name, pr_number, title, author, head_sha, status, priority, installation_id, queued_at) VALUES ('test-org', 'test-repo', {}, 'Test PR', 'test-author', '{}', 'queued', 0, 12345, datetime('now'));",
        pr_number, head_sha
    );
    let output = Command::new("sqlite3")
        .arg(db_path)
        .arg(&sql)
        .output()
        .expect("sqlite3 must be available");
    assert!(
        output.status.success(),
        "sqlite3 insert failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn comment_payload(body: &str, pr_number: i32) -> serde_json::Value {
    serde_json::json!({
        "action": "created",
        "installation": { "id": 12345 },
        "repository": {
            "full_name": "test-org/test-repo",
            "owner": { "login": "test-org" },
            "name": "test-repo"
        },
        "issue": {
            "number": pr_number,
            "pull_request": { "url": "https://api.github.com/repos/test-org/test-repo/pulls/10" }
        },
        "comment": {
            "body": body,
            "user": { "login": "test-reviewer" }
        }
    })
}

fn pr_event_payload(action: &str, pr_number: i32, head_sha: &str) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "installation": { "id": 12345 },
        "repository": {
            "full_name": "test-org/test-repo",
            "owner": { "login": "test-org" },
            "name": "test-repo"
        },
        "pull_request": {
            "number": pr_number,
            "title": "Test PR",
            "head": {
                "sha": head_sha,
                "ref": "feature-branch"
            },
            "user": { "login": "test-author" },
            "state": "open",
            "mergeable": true,
            "labels": []
        }
    })
}

#[tokio::test]
async fn test_comment_cancel_dequeues_pr() {
    let db_path = next_db_path();
    let client = build_test_app(&db_path).await;
    insert_test_pr(&db_path, 10, "abc123");

    let payload = comment_payload("@fila cancel", 10);
    let signature = sign_payload(&payload);

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "issue_comment")
        .header("X-Hub-Signature-256", &signature)
        .json(&payload)
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = client.get("/pull_requests").send().await;
    let prs: Vec<serde_json::Value> = res.json();
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["status"], "cancelled");
}

#[tokio::test]
async fn test_pr_close_dequeues() {
    let db_path = next_db_path();
    let client = build_test_app(&db_path).await;
    insert_test_pr(&db_path, 20, "abc123");

    let payload = pr_event_payload("closed", 20, "abc123");
    let signature = sign_payload(&payload);

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "pull_request")
        .header("X-Hub-Signature-256", &signature)
        .json(&payload)
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = client.get("/pull_requests").send().await;
    let prs: Vec<serde_json::Value> = res.json();
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["status"], "cancelled");
}

#[tokio::test]
async fn test_pr_sync_updates_sha() {
    let db_path = next_db_path();
    let client = build_test_app(&db_path).await;
    insert_test_pr(&db_path, 30, "abc123");

    let payload = pr_event_payload("synchronize", 30, "new-sha-456");
    let signature = sign_payload(&payload);

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "pull_request")
        .header("X-Hub-Signature-256", &signature)
        .json(&payload)
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = client.get("/pull_requests").send().await;
    let prs: Vec<serde_json::Value> = res.json();
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["head_sha"], "new-sha-456");
    assert_eq!(prs[0]["status"], "queued");
}

#[tokio::test]
async fn test_comment_on_issue_ignored() {
    let db_path = next_db_path();
    let client = build_test_app(&db_path).await;

    let payload = serde_json::json!({
        "action": "created",
        "installation": { "id": 12345 },
        "repository": {
            "full_name": "test-org/test-repo",
            "owner": { "login": "test-org" },
            "name": "test-repo"
        },
        "issue": {
            "number": 50
        },
        "comment": {
            "body": "@fila ship",
            "user": { "login": "test-reviewer" }
        }
    });
    let signature = sign_payload(&payload);

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "issue_comment")
        .header("X-Hub-Signature-256", &signature)
        .json(&payload)
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = client.get("/pull_requests").send().await;
    let prs: Vec<serde_json::Value> = res.json();
    assert_eq!(prs.len(), 0);
}

#[tokio::test]
async fn test_random_comment_ignored() {
    let db_path = next_db_path();
    let client = build_test_app(&db_path).await;

    let payload = comment_payload("looks good to me!", 10);
    let signature = sign_payload(&payload);

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "issue_comment")
        .header("X-Hub-Signature-256", &signature)
        .json(&payload)
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = client.get("/pull_requests").send().await;
    let prs: Vec<serde_json::Value> = res.json();
    assert_eq!(prs.len(), 0);
}

#[tokio::test]
async fn test_webhook_rejects_missing_signature() {
    let db_path = next_db_path();
    let client = build_test_app(&db_path).await;

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "issue_comment")
        .json(&comment_payload("@fila cancel", 10))
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_webhook_rejects_invalid_signature() {
    let db_path = next_db_path();
    let client = build_test_app(&db_path).await;

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "issue_comment")
        .header("X-Hub-Signature-256", "sha256=deadbeef")
        .json(&comment_payload("@fila cancel", 10))
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_dashboard_returns_html() {
    let db_path = next_db_path();
    let client = build_test_app(&db_path).await;

    let res = client.get("/").send().await;
    assert_eq!(res.status(), StatusCode::OK);

    let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("text/html"));

    let body = res.text();
    assert!(body.contains("Fila"));
    assert!(body.contains("Merge Queue"));
}
