use std::sync::atomic::{AtomicU32, Ordering};

use rapina::prelude::*;
use rapina::testing::TestClient;

use fila::config::app::AppConfig;

static TEST_DB_COUNTER: AtomicU32 = AtomicU32::new(0);

async fn build_test_app() -> TestClient {
    let id = TEST_DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let db_path = format!("/tmp/fila_test_{}.db", id);
    let _ = std::fs::remove_file(&db_path);

    let config = AppConfig {
        database_url: format!("sqlite://{}?mode=rwc", db_path),
        server_port: 0,
        host: "127.0.0.1".to_string(),
        github_app_id: "test-app-id".to_string(),
        github_private_key: "test-private-key".to_string(),
        github_webhook_secret: "test-secret".to_string(),
        batch_size: 5,
        batch_interval_secs: 300,
    };

    let app = fila::build_app(config, false).await;
    TestClient::new(app).await
}

fn webhook_payload(action: &str, label: &str, pr_number: i32) -> serde_json::Value {
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
                "sha": "abc123",
                "ref": "feature-branch"
            },
            "user": { "login": "test-author" },
            "state": "open",
            "mergeable": true,
            "labels": []
        },
        "label": { "name": label }
    })
}

#[tokio::test]
async fn test_webhook_enqueues_pr() {
    let client = build_test_app().await;

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "pull_request")
        .json(&webhook_payload("labeled", "merge", 42))
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = client.get("/pull_requests").send().await;
    assert_eq!(res.status(), StatusCode::OK);

    let prs: Vec<serde_json::Value> = res.json();
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["pr_number"], 42);
    assert_eq!(prs[0]["status"], "queued");
    assert_eq!(prs[0]["author"], "test-author");
    assert_eq!(prs[0]["repo_owner"], "test-org");
}

#[tokio::test]
async fn test_webhook_dequeues_on_unlabel() {
    let client = build_test_app().await;

    client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "pull_request")
        .json(&webhook_payload("labeled", "merge", 10))
        .send()
        .await;

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "pull_request")
        .json(&webhook_payload("unlabeled", "merge", 10))
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = client.get("/pull_requests").send().await;
    let prs: Vec<serde_json::Value> = res.json();
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["status"], "cancelled");
}

#[tokio::test]
async fn test_webhook_dequeues_on_close() {
    let client = build_test_app().await;

    client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "pull_request")
        .json(&webhook_payload("labeled", "merge", 20))
        .send()
        .await;

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "pull_request")
        .json(&webhook_payload("closed", "", 20))
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = client.get("/pull_requests").send().await;
    let prs: Vec<serde_json::Value> = res.json();
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0]["status"], "cancelled");
}

#[tokio::test]
async fn test_webhook_updates_sha_on_sync() {
    let client = build_test_app().await;

    client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "pull_request")
        .json(&webhook_payload("labeled", "merge", 30))
        .send()
        .await;

    let mut payload = webhook_payload("synchronize", "", 30);
    payload["pull_request"]["head"]["sha"] = serde_json::json!("new-sha-456");

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "pull_request")
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
async fn test_webhook_ignores_non_merge_label() {
    let client = build_test_app().await;

    let res = client
        .post("/webhooks/github")
        .header("X-GitHub-Event", "pull_request")
        .json(&webhook_payload("labeled", "bug", 50))
        .send()
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = client.get("/pull_requests").send().await;
    let prs: Vec<serde_json::Value> = res.json();
    assert_eq!(prs.len(), 0);
}

#[tokio::test]
async fn test_dashboard_returns_html() {
    let client = build_test_app().await;

    let res = client.get("/").send().await;
    assert_eq!(res.status(), StatusCode::OK);

    let content_type = res
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/html"));

    let body = res.text();
    assert!(body.contains("Fila"));
    assert!(body.contains("Merge Queue"));
}
