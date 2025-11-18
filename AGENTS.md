# Repository Guidelines

## Project Structure & Module Organization
Repository tree mirrors a standard Cargo layout: `src/` contains the Rust port of sshpass with focused modules such as `cli.rs`, `monitor.rs`, `pty.rs`, `process.rs`, and `password.rs`; `sshpass-1.10/` houses the original C reference for parity checks. Place integration tests in `tests/`, quick demonstrations in `examples/`, and keep all generated artifacts confined to `target/`. Root-level documents (`README.md`, `DEVELOP.md`, `TODO.md`, etc.) record design intent—update them whenever behavior changes.

## Build, Test, and Development Commands
- `cargo check` — fast type-check to validate edits.
- `cargo fmt && cargo clippy --all-targets` — enforce formatting and lint rules before review.
- `cargo test` — run the full Unix-only suite (63 tests, ~85% coverage).
- `cargo build --release` — produce the optimized binary at `target/release/sshpass`.
- `cargo run -- <sshpass args>` — smoke-test CLI changes locally.

## Coding Style & Naming Conventions
Use Rust 2021 idioms with 4-space indentation and snake_case identifiers for functions, modules, and files; reserve CamelCase for types and enums. All Rust code must pass `cargo fmt` (rustfmt defaults) and be lint-clean under `clippy -D warnings`. Keep modules cohesive: CLI parsing logic in `cli.rs`, terminal control inside `pty.rs`, and side-effectful helpers behind dedicated structs to simplify testing.

## Testing Guidelines
Leverage Rust’s built-in test framework plus integration harnesses in `tests/`. Prefer descriptive `test_*` function names (e.g., `test_password_from_env`). When reproducing SSH flows, isolate external dependencies behind fakes so tests remain deterministic. Maintain ≥85% coverage by adding regression cases for each bug fix, and document non-trivial fixtures in `TESTING.md`.

## Commit & Pull Request Guidelines
The repository currently lacks historical commits; adopt Conventional Commits (`feat:`, `fix:`, `chore:`, etc.) so history remains searchable. Every pull request should summarize intent, link related TODO items, describe testing performed (`cargo test`, linting, manual SSH flow), and include screenshots or terminal captures when affecting user-visible behavior. Reference updated docs in the PR body whenever applicable.

## Security & Configuration Tips
Avoid embedding secrets in the repo; when demonstrating passwords, rely on environment variables or FIFOs that are destroyed post-run. Verify `sshpass` behavior only on Unix-like hosts with PTY support, and keep sample password files at `chmod 600`. When adding new functionality, audit interactions with `nix` and `signal-hook` crates to ensure signals and file descriptors are safely handled.
