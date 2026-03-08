# Fila — GitHub Merge Queue

Single Rust binary built with Rapina. Receives GitHub webhooks, queues PRs,
merges them into fila/merge staging branch, waits for CI, fast-forwards main.

## Architecture

webhook.rs → queue/service.rs → queue/runner.rs → github/client.rs

## Domain model

- PrStatus: Queued → Testing → Merged | Failed | Cancelled
- BatchStatus: Pending → Testing → Done | Failed
- Commands: @fila ship, @fila cancel, @fila status
- Strategies: batch (default), sequential

## Key files

- src/github/webhook.rs — webhook router, HMAC-verified, handles all GitHub events
- src/github/signature.rs — SignedBody<T> extractor for HMAC-SHA256 verification
- src/github/client.rs — GitHub API (JWT auth, token cache, merges, refs, comments)
- src/queue/service.rs — queue operations (enqueue, dequeue, mark_*, log_event)
- src/queue/runner.rs — background loop, batch and sequential strategies
- src/dashboard.rs — HTML dashboard (public, no auth)
- src/entity.rs — PullRequest, Batch, MergeEvent schemas
- src/config/app.rs — env-driven config
- src/types.rs — PrStatus, BatchStatus enums
- src/errors.rs — shared CrudError for CRUD handlers

## Conventions

- Feature-first modules (plural: pull_requests/, batches/, merge_events/)
- Status transitions via enums in types.rs, never raw strings
- Queue service functions: take &Db, return Result<_, Error>
- Runner has its own DB connection, spawns as tokio task
- All routes auto-discovered via .discover()
- Public routes: GET / (dashboard), POST /webhooks/github
- Webhooks are HMAC-SHA256 verified via SignedBody extractor

## Testing

cargo test — integration tests use TestClient + sqlite3
Tests in tests/integration.rs
Webhook tests include HMAC signature generation

## Environment

DATABASE_URL, GITHUB_APP_ID, GITHUB_PRIVATE_KEY, GITHUB_WEBHOOK_SECRET,
MERGE_STRATEGY (batch|sequential), BATCH_SIZE, BATCH_INTERVAL_SECS,
CI_TIMEOUT_SECS, POLL_INTERVAL_SECS, DASHBOARD_URL
