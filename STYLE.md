# Code Style

This file defines the Rust style contract for ErsatzRS. It is intentionally stricter
than the current codebase requires because most code has not been written yet.

## Baseline

- Use stable Rust through [rust-toolchain.toml](rust-toolchain.toml).
- Keep the MSRV policy at "current stable minus one" once releases begin.
- Use Rust 2021 until an ADR upgrades the edition.
- Prefer explicit modules and small files over large catch-all modules.
- Keep public APIs documented in library crates.

## Naming

- Crates use `ErsatzRS-*`.
- Modules, functions, variables, and fields use `snake_case`.
- Types, traits, and enum variants use `PascalCase`.
- Constants use `SCREAMING_SNAKE_CASE`.
- Database identifiers use `snake_case`.

## Errors

- Library crates use `thiserror` for typed errors.
- Binaries and top-level entry points may use `anyhow`.
- Use `Result<T, E>` for recoverable failures.
- Avoid panics in library code.
- `unwrap` and `expect` are allowed only in tests, doctests, and startup code
  where failure is unrecoverable and the message explains the invariant.

## Async

- Use Tokio for async runtime behavior.
- Do not block inside async code. Use `spawn_blocking` for blocking filesystem or
  CPU-heavy work when needed.
- Pass cancellation-aware handles where long-running FFmpeg, scanner, or
  scheduling operations are involved.

## Logging

- Use `tracing`, not `println!`, in library code.
- Log operational state with fields, not string interpolation alone.
- Never log secrets, bearer tokens, cookies, database passwords, or full config
  dumps.
- Follow [docs/how-to/logging.md](docs/how-to/logging.md).

## Public API Documentation

Public items in `ersatzrs-core`, `ersatzrs-ffmpeg`, and `ersatzrs-app` must include
rustdoc. Prefer examples for stable APIs and explain invariants for builders,
state machines, and generated FFmpeg arguments.

## Unsafe Code

Unsafe Rust is denied by default. Any future exception requires an ADR and must
be isolated behind a safe API.

## Dependencies

- Prefer small, maintained crates with clear licenses.
- Add cross-crate dependencies only when the owning crate boundary still matches
  [ARCHITECTURE.md](ARCHITECTURE.md).
- Use [deny.toml](deny.toml) to enforce license and advisory policy once the
  workspace exists.
