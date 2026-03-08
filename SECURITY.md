# Security Policy

## Reporting a vulnerability

If you discover a security vulnerability in Fila, please report it privately. Do not open a public issue.

Email **hello@zaptech.dev** with:

- A description of the vulnerability
- Steps to reproduce
- Potential impact

We will acknowledge your report within 48 hours and work on a fix. Once resolved, we'll credit you in the release notes (unless you prefer to remain anonymous).

## Scope

Fila handles GitHub webhook payloads and authenticates as a GitHub App using private keys and JWTs. Security-relevant areas include:

- Webhook signature verification (HMAC-SHA256)
- GitHub App authentication (JWT generation, installation tokens)
- API request handling and input validation
- SQLite query construction

## Supported versions

Only the latest release is supported with security updates.
