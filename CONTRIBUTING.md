# Contributing

NasTv starts as a solo-maintainer project, but the repository is structured as a
future public open-source project from day one. Contributions should leave a
clear trail: what changed, why it changed, how it was tested, and whether it
changes the ErsatzTV parity contract.

## Ground Rules

- Use English for repository content.
- Keep changes small and reviewable.
- Preserve ErsatzTV behavior unless an ADR records a deliberate deviation.
- Prefer contracts already in this repository over ad hoc decisions.
- Do not introduce secrets, tokens, private media paths, or personal library data
  into source, fixtures, logs, snapshots, or documentation.

## Development Setup

The Rust workspace has not been created yet. Once it exists, the baseline setup
is:

```bash
rustup show
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

FFmpeg 8.1 or newer is required at runtime because NasTv uses the `drawvg`
filter. Startup code must fail fast when an older FFmpeg is detected.

## Workflow

1. Read the relevant contract first:
   - [STYLE.md](STYLE.md) for code.
   - [ARCHITECTURE.md](ARCHITECTURE.md) for crate boundaries.
   - [PORTING.md](PORTING.md) for ErsatzTV parity work.
   - [docs/how-to/testing.md](docs/how-to/testing.md) for test placement.
   - [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for documentation.
2. For porting work, read the ErsatzTV source before writing Rust.
3. Add or update tests at the narrowest useful level.
4. Update [CHANGELOG.md](CHANGELOG.md) under `[Unreleased]`.
5. Update [docs/reference/parity-matrix.md](docs/reference/parity-matrix.md)
   when a feature changes parity status.
6. Add an ADR when required by [docs/adr/0001-record-architecture-decisions.md](docs/adr/0001-record-architecture-decisions.md).

## Commit Messages

Use Conventional Commits:

```text
type(scope): short imperative summary
```

Allowed types:

- `feat`
- `fix`
- `docs`
- `style`
- `refactor`
- `test`
- `build`
- `ci`
- `perf`
- `chore`
- `revert`

Preferred scopes:

- `domain`
- `core`
- `ffmpeg`
- `infra`
- `app`
- `web`
- `scanner`
- `docs`
- `ci`
- `deps`
- `release`

Examples:

```text
docs(porting): add ErsatzTV idiom mapping
feat(ffmpeg): add software pipeline argv builder
test(scheduling): snapshot yaml duration instruction
```

## Pull Request Checklist

- The change is in English.
- The relevant contract files were followed.
- Tests were added or the absence of tests is explained.
- Snapshots were reviewed when generated output changed.
- `cargo fmt`, `cargo clippy`, `cargo test`, and `cargo deny check` were run
  once the workspace exists.
- `CHANGELOG.md` was updated.
- The parity matrix was updated for ported behavior.
- An ADR is linked when the change affects architecture or deviates from
  ErsatzTV behavior.

## Review Expectations

Reviews prioritize correctness, maintainability, parity risk, test coverage, and
operational safety. Style feedback should point to an existing contract or be
converted into one if it will matter again.
