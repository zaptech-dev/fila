# Fila

A merge queue for GitHub, inspired by [bors](https://github.com/rust-lang/bors). Fila ensures your main branch only contains code that has been tested as a merge result — never untested combinations.

"Fila" means "queue" in Portuguese 🇧🇷

## Why

GitHub's merge button has a fundamental problem: it merges code that was tested in isolation, not as the combination that will actually land on main. If PR #1 and PR #2 both pass CI independently but break when combined, you'll only find out after both are on main.

Fila solves this by testing the exact merge result before pushing to main. Every commit on main has passed CI in the exact state it will be deployed.

## How it works

```
  PR #1 ─┐
  PR #2 ─┤   ┌──────────┐     ┌─────┐     ┌──────┐
  PR #3 ─┼──▶│fila/merge │────▶│ CI  │────▶│ main │
  PR #4 ─┤   └──────────┘     └─────┘     └──────┘
  PR #5 ─┘   merge each        1 run      fast-forward
```

1. Someone comments `@fila ship` on a PR (or includes it in a review)
2. Fila adds the PR to its queue and confirms with a comment
3. The runner picks all queued PRs and creates a temporary `fila/merge` branch from current `main`
4. It merges each PR sequentially into `fila/merge`, producing the exact combined commit that would land on main
5. CI runs once on `fila/merge` — testing all PRs together, not in isolation
6. If CI passes, Fila fast-forwards `main` to the tested commit — one build for all queued PRs
7. If CI fails, all PRs in the batch are marked as failed and the authors are notified
8. If a PR has a merge conflict, it's skipped and marked as failed — the rest of the batch continues

### Merge strategies

Fila supports two merge strategies, controlled by the `MERGE_STRATEGY` environment variable:

**`batch`** (default) — Merge all queued PRs into `fila/merge` together, run CI once, fast-forward main once. This saves CI runs and deploy costs. If you have 10 PRs queued, that's 1 build instead of 10. The tradeoff: if CI fails, you can't tell which PR broke it — all get failed and authors retry with `@fila ship`.

**`sequential`** — Process one PR at a time, like bors. Each PR is tested against the exact main it will land on. Slower but guarantees isolation. Use this when you need to know exactly which PR broke the build.

## Commands

Comment on any PR to interact with Fila:

| Command | Description |
|---------|-------------|
| `@fila ship` | Add the PR to the merge queue |
| `@fila cancel` | Remove the PR from the queue |
| `@fila status` | Show the current queue |

`@fila ship` works in both regular comments and PR review bodies — approve and ship in one step.

## Installation

### 1. Create a GitHub App

Go to **Settings > Developer settings > GitHub Apps > New GitHub App**.

**Permissions (Repository):**

| Permission | Access |
|------------|--------|
| Contents | Read & write |
| Issues | Read & write |
| Pull requests | Read & write |
| Commit statuses | Read & write |
| Checks | Read |

**Subscribe to events:** Check suite, Issue comment, Pull request, Pull request review

Generate a private key and note the App ID.

### 2. Install the app

Install the GitHub App on the repositories you want Fila to manage.

### 3. Branch protection

If your `main` branch has branch protection rules (required PRs, required reviews, etc.), you need to add the GitHub App to the **bypass list** so it can fast-forward main after CI passes. Without this, the fast-forward will be rejected by GitHub.

Go to **Settings > Branches > main > Edit** and add your app under "Allow specified actors to bypass required pull requests". Or via API:

```bash
gh api repos/OWNER/REPO/branches/main/protection -X PUT --input - <<'EOF'
{
  "required_pull_request_reviews": {
    "required_approving_review_count": 1,
    "bypass_pull_request_allowances": {
      "apps": ["your-app-slug"]
    }
  },
  "enforce_admins": true,
  "restrictions": null,
  "required_status_checks": null
}
EOF
```

### 4. Configure CI

Your CI workflow must run on the `fila/merge` branch. This is the branch where Fila tests merge results:

```yaml
on:
  push:
    branches: [main, fila/merge]
  pull_request:
    branches: [main]
```

### 5. Run Fila

Fila is a single binary that connects to GitHub via webhooks and stores state in SQLite.

**Environment variables:**

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | SQLite connection string | required |
| `SERVER_PORT` | HTTP port | `8000` |
| `GITHUB_APP_ID` | GitHub App ID | required |
| `GITHUB_PRIVATE_KEY` | GitHub App private key (PEM contents) | required |
| `GITHUB_WEBHOOK_SECRET` | Webhook secret | required |
| `MERGE_STRATEGY` | `batch` or `sequential` | `batch` |
| `BATCH_SIZE` | Max PRs per batch | `5` |
| `BATCH_INTERVAL_SECS` | How often to check for queued PRs (seconds) | `10` |
| `CI_TIMEOUT_SECS` | Max time to wait for CI (seconds) | `1800` |
| `POLL_INTERVAL_SECS` | How often to poll CI status (seconds) | `15` |
| `DASHBOARD_URL` | URL for queue link in PR comments | — |

```bash
# .env
DATABASE_URL=sqlite://fila.db?mode=rwc
GITHUB_APP_ID=12345
GITHUB_PRIVATE_KEY="-----BEGIN RSA PRIVATE KEY-----\n...\n-----END RSA PRIVATE KEY-----"
GITHUB_WEBHOOK_SECRET=your-secret
```

```bash
cargo run
```

The dashboard is available at `http://localhost:8000/`.

### Deploy

**Docker:**

```dockerfile
FROM rust:1.88-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/fila /usr/local/bin/fila
CMD ["fila"]
```

**Railway:**

1. Connect your repository
2. Add a volume mounted at `/data` for SQLite persistence
3. Set `DATABASE_URL=sqlite:///data/fila.db?mode=rwc` and the other env vars
4. Set the webhook URL to `https://your-app.up.railway.app/webhooks/github`

## Architecture

Fila is a single Rust binary built with [Rapina](https://github.com/rapina-rs/rapina).

```
┌─────────────────────────────────────────────────┐
│                    Fila                         │
│                                                 │
│  ┌──────────┐  ┌──────────────┐  ┌───────────┐ │
│  │ Webhooks │  │ Merge Queue  │  │ Dashboard │ │
│  │ (GitHub) │  │   Runner     │  │  (HTML)   │ │
│  └────┬─────┘  └──────┬───────┘  └───────────┘ │
│       │               │                        │
│       └───────┬───────┘                        │
│               ▼                                │
│         ┌──────────┐      ┌──────────────┐     │
│         │  SQLite  │      │  GitHub API  │     │
│         └──────────┘      └──────────────┘     │
└─────────────────────────────────────────────────┘
```

The default batch strategy merges all queued PRs together and runs CI once, saving build time and deploy costs. For teams that need strict isolation, the sequential strategy processes one PR at a time.

## Development

```bash
# Run tests
cargo test

# Run with hot reload
cargo watch -x run

# Check formatting and lints
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

## License

MIT
