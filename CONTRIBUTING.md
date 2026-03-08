# Contributing

Fila is open source and contributions are welcome.

## Getting started

```bash
git clone https://github.com/zaptech-dev/fila.git
cd fila
cargo test
```

You'll need Rust 1.88+ and `sqlite3` (for integration tests).

## Making changes

1. Fork the repo and create a branch from `main`
2. Write your code — run `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings`
3. Add tests if applicable
4. Open a pull request against `main`

## What we're looking for

Bug fixes, performance improvements, better error messages, new merge strategies, dashboard improvements, and documentation. If you're planning something large, open an issue first so we can discuss the approach.

## Code style

Rust conventions apply. Run `cargo fmt` before committing. No warnings from `clippy`. Keep functions small, errors explicit, and abstractions minimal.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
