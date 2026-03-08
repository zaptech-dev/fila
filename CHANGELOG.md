# Changelog

All notable changes to Fila will be documented in this file.

## Unreleased

### Added

- Batch merge strategy — merge all queued PRs together, run CI once, fast-forward main once
- Sequential merge strategy — process one PR at a time (bors-style)
- `MERGE_STRATEGY` config to switch between `batch` (default) and `sequential`
- Queue confirmation comments when PRs are added or removed via `@fila ship` / `@fila cancel`
- `@fila ship` support in PR review bodies (not just issue comments)
- Dashboard link in queue confirmation comments (`DASHBOARD_URL` config)
- Explicit PR closure after successful merge
- `@fila status` command to show current queue
- Dockerfile for containerized deployment
- Railway deployment support with persistent volume for SQLite

## 0.1.0 — 2026-03-07

Initial release.

- GitHub App webhook handler for issue comments, pull requests, check suites, and reviews
- Merge queue with priority ordering
- `fila/merge` branch for testing merge results before landing on main
- CI polling with configurable timeout and interval
- SQLite storage for queue state, batch history, and merge events
- Web dashboard
- Built with [Rapina](https://github.com/rapina-rs/rapina)
