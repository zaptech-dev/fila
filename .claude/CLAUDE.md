# Fila — GitHub Merge Queue

Single Rust binary built with Rapina. Receives GitHub webhooks, queues PRs,
merges them into a fila/merge staging branch, waits for CI, fast-forwards main.

## Architecture

```
GitHub webhook → src/github/webhook.rs (HMAC verified)
                    ↓
              src/queue/service.rs (enqueue, dequeue, status transitions)
                    ↓
              src/queue/runner.rs (background tokio task, polls on interval)
                    ↓
              src/github/client.rs (GitHub API: merges, refs, comments, checks)
```

## Domain model

Status enums live in `src/types.rs`:
- `PrStatus`: Queued → Testing → Merged | Failed | Cancelled
- `BatchStatus`: Pending → Testing → Done | Failed
- Always use enum variants, never raw strings for statuses

Commands (via GitHub comments or review body):
- `@fila ship` — add PR to merge queue
- `@fila cancel` — remove PR from queue
- `@fila status` — show current queue

Merge strategies (via MERGE_STRATEGY env):
- `batch` (default) — merge all queued PRs together, run CI once, fast-forward once
- `sequential` — one PR at a time (bors-style)

## Key files

- `src/github/webhook.rs` — webhook router, handles PR/comment/review/check_suite events
- `src/github/signature.rs` — `SignedBody<T>` extractor, HMAC-SHA256 verification
- `src/github/client.rs` — GitHub API client (JWT auth, installation token caching, merge/ref/comment operations)
- `src/github/types.rs` — GitHub API and webhook payload type definitions
- `src/queue/service.rs` — queue operations (enqueue, dequeue, mark_testing/merged/failed, create_batch, log_event)
- `src/queue/runner.rs` — background merge loop, batch and sequential strategies, CI polling
- `src/dashboard.rs` — HTML dashboard at GET / (public, no auth)
- `src/entity.rs` — SeaORM schema: PullRequest, Batch, MergeEvent
- `src/config/app.rs` — AppConfig with env-driven loading
- `src/types.rs` — PrStatus, BatchStatus enums with Display/AsRef/From impls
- `src/errors.rs` — shared CrudError for all CRUD handler modules
- `src/lib.rs` — build_app() takes AppConfig + Arc<GitHubClient>, runs migrations, discovers routes
- `src/main.rs` — creates GitHubClient once, spawns runner, starts server

## Project structure

```
src/
├── main.rs              # Entry point, single GitHubClient creation
├── lib.rs               # build_app(config, github, enable_tracing)
├── entity.rs            # PullRequest, Batch, MergeEvent (schema! macro)
├── types.rs             # PrStatus, BatchStatus enums
├── errors.rs            # CrudError (shared by all CRUD handlers)
├── dashboard.rs         # GET / — HTML dashboard
├── config/
│   └── app.rs           # AppConfig (env vars)
├── github/
│   ├── client.rs        # GitHubClient (JWT, tokens, API calls)
│   ├── types.rs         # GitHub API types, webhook payloads
│   ├── webhook.rs       # POST /webhooks/github handler
│   └── signature.rs     # SignedBody<T> HMAC extractor
├── queue/
│   ├── service.rs       # Queue operations
│   └── runner.rs        # Background merge runner
├── pull_requests/
│   └── handlers.rs      # GET /pull_requests, GET /pull_requests/:id
├── batches/
│   └── handlers.rs      # GET /batches, GET /batches/:id
├── merge_events/
│   └── handlers.rs      # GET /merge_events, GET /merge_events/:id
└── migrations/          # SeaORM migrations
```

## Conventions

- Feature-first modules (plural: pull_requests/, batches/, merge_events/)
- Status transitions via enums, never raw strings
- Queue service functions take `&Db`, return `Result<_, Error>`
- Runner has its own DB connection, spawns as tokio task
- CRUD handlers use shared `CrudError` (no per-module error types)
- All routes auto-discovered via `.discover()`
- Public routes: `GET /` (dashboard), `POST /webhooks/github`
- Webhooks verified via `SignedBody<T>` extractor (HMAC-SHA256)
- Single `GitHubClient` created in main.rs, shared via `Arc`

## Testing

```bash
cargo test
```

Integration tests in `tests/integration.rs`:
- Use `TestClient` + sqlite3 for test data
- Webhook tests generate HMAC signatures with `sign_payload()` helper
- Test secret: "test-secret"

## Environment variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| DATABASE_URL | yes | — | SQLite connection string |
| GITHUB_APP_ID | yes | — | GitHub App ID |
| GITHUB_PRIVATE_KEY | yes | — | GitHub App private key (PEM) |
| GITHUB_WEBHOOK_SECRET | yes | — | HMAC secret for webhook verification |
| MERGE_STRATEGY | no | batch | batch or sequential |
| BATCH_SIZE | no | 5 | Max PRs per batch |
| BATCH_INTERVAL_SECS | no | 10 | Seconds between runner cycles |
| CI_TIMEOUT_SECS | no | 1800 | Max seconds to wait for CI |
| POLL_INTERVAL_SECS | no | 15 | Seconds between CI poll checks |
| DASHBOARD_URL | no | "" | URL for queue links in PR comments |

## Build & Run

```bash
cargo build --release
cargo test
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```
