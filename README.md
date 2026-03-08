# Fila

A merge queue for GitHub, inspired by [bors](https://github.com/rust-lang/bors). Fila ensures your main branch only contains code that has been tested as a merge result — never untested combinations.

"Fila" means "queue" in Portuguese.

## Why

GitHub's merge button has a fundamental problem: it merges code that was tested in isolation, not as the combination that will actually land on main. If PR #1 and PR #2 both pass CI independently but break when combined, you'll only find out after both are on main.

Fila solves this by testing the exact merge result before pushing to main. Every commit on main has passed CI in the exact state it will be deployed.

## How it works

When someone comments `@fila ship` on a PR:

1. Fila adds the PR to its queue
2. The runner picks the next PR and creates a temporary `fila/merge` branch from current `main`
3. It merges the PR into `fila/merge`, producing the exact commit that would land on main
4. CI runs on `fila/merge` — testing the combined result, not the PR in isolation
5. If CI passes, Fila fast-forwards `main` to that tested commit
6. If CI fails, the PR is marked as failed and the author is notified

This is the same approach used by the Rust compiler's merge queue. One PR at a time, each tested against the latest main.

## Commands

Comment on any PR to interact with Fila:

| Command | Description |
|---------|-------------|
| `@fila ship` | Add the PR to the merge queue |
| `@fila cancel` | Remove the PR from the queue |
| `@fila status` | Show the current queue |

## Installation

### 1. Create a GitHub App

Go to **Settings > Developer settings > GitHub Apps > New GitHub App**.

**Permissions (Repository):**
- Contents: Read & write
- Issues: Read & write
- Pull requests: Read & write
- Commit statuses: Read & write
- Checks: Read

**Subscribe to events:**
- Check suite
- Issue comment
- Pull request

Generate a private key and note the App ID.

### 2. Install the app

Install the GitHub App on the repositories you want Fila to manage.

### 3. Configure CI

Your CI workflow must run on the `fila/merge` branch. This is the branch where Fila tests merge results:

```yaml
on:
  push:
    branches: [main, fila/merge]
  pull_request:
    branches: [main]
```

### 4. Run Fila

Fila is a single binary that connects to GitHub via webhooks and stores state in SQLite.

**Environment variables:**

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | SQLite connection string | required |
| `SERVER_PORT` | HTTP port | `8000` |
| `GITHUB_APP_ID` | GitHub App ID | required |
| `GITHUB_PRIVATE_KEY` | GitHub App private key (PEM contents) | required |
| `GITHUB_WEBHOOK_SECRET` | Webhook secret | required |
| `BATCH_INTERVAL_SECS` | How often to check for queued PRs | `10` |
| `CI_TIMEOUT_SECS` | Max time to wait for CI | `1800` |
| `POLL_INTERVAL_SECS` | How often to poll CI status | `15` |

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

### Docker

```dockerfile
FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/fila /usr/local/bin/fila
CMD ["fila"]
```

### Railway

1. Connect your repository
2. Set the environment variables in Railway's dashboard
3. Set the webhook URL to your Railway deployment URL + `/webhooks/github`

## Architecture

Fila is a single Rust binary built with [Rapina](https://github.com/rapina-rs/rapina).

- **Web server** — receives GitHub webhooks, serves the dashboard
- **Merge queue runner** — background task that processes the queue
- **SQLite** — stores queue state, batch history, and merge events
- **GitHub API** — creates merge commits, polls CI, fast-forwards main

The flow is intentionally simple: one PR at a time, sequential processing, no batching. This makes the system predictable and easy to debug.

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
