# Contributing

ErsatzRS starts as a solo-maintainer project, but the repository is structured as a
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
- Do not commit absolute filesystem paths from a maintainer's machine
  (`/home/<user>/...`, `/Users/<user>/...`, `C:\Users\...`). Use repo-relative
  paths for ErsatzRS, and the documented `$ERSATZTV_SRC` env var for upstream
  ErsatzTV source — see [PORTING.md](PORTING.md).

## Development Setup

Rust `1.97.1` is the minimum supported toolchain and is selected automatically
by the checked-in `rust-toolchain.toml`. The baseline setup is:

```bash
rustup show
cargo fmt --all -- --check
# Lint mirrors CI: two passes, neither with --all-features, so feature-gated
# test files do not escape lint.
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
./run.sh test-closeout
cargo deny check
```

FFmpeg 8.1 or newer is required at runtime because ErsatzRS uses the `drawvg`
filter. Startup code must fail fast when an older FFmpeg is detected.

For the edit loop, install cargo-nextest using its
[official prebuilt-binary instructions](https://nexte.st/docs/installation/pre-built-binaries/)
and use `./run.sh test -p <crate>` for focused/local tests. Agent command hooks
reject direct `cargo test` invocations so the runner choice cannot silently
drift. The repository profile reports the complete failure set without retries
and marks tests slow after 60 seconds. Use `./run.sh test-closeout` for the
canonical local gate: it runs nextest across the workspace, then invokes Cargo
only for doctests. CI remains on its existing direct Cargo test selection.

## Workflow

1. Read the relevant contract first:
   - [STYLE.md](STYLE.md) for code.
   - [ARCHITECTURE.md](ARCHITECTURE.md) for crate boundaries.
   - [PORTING.md](PORTING.md) for ErsatzTV parity work.
   - [docs/how-to/testing.md](docs/how-to/testing.md) for test placement.
   - [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for documentation.
2. For porting work, read the ErsatzTV source before writing Rust.
3. Add or update tests at the narrowest useful level.
4. Update [CHANGELOG.md](CHANGELOG.md) under `[Unreleased]` for every notable
   user-visible, parity-contract, runtime/build, dependency, security, release,
   or documented behavior change.
5. Update the feature's `docs/progress/parity/<slug>.toml` when its parity
   status changes (see the `update-parity` skill), then run
   `cargo xtask progress check`.
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
- `status`

Examples:

```text
docs(porting): add ErsatzTV idiom mapping
feat(ffmpeg): add software pipeline argv builder
test(scheduling): snapshot yaml duration instruction
```

Map commit intent to changelog categories when writing the entry: `feat`
usually belongs under `Added`, `fix` under `Fixed`, behavior changes under
`Changed`, removals under `Removed`, deprecations under `Deprecated`, and
vulnerability work under `Security`. Breaking changes must be called out
explicitly in the relevant changelog entry.

## Pull Request Checklist

- The change is in English.
- The relevant contract files were followed.
- Tests were added or the absence of tests is explained.
- Snapshots were reviewed when generated output changed.
- `cargo fmt`, `cargo clippy`, `./run.sh test-closeout`, and `cargo deny check`
  were run.
- `CHANGELOG.md` was updated.
- `docs/progress/` parity files were updated for ported behavior.
- An ADR is linked when the change affects architecture or deviates from
  ErsatzTV behavior.

## Review Expectations

Reviews prioritize correctness, maintainability, parity risk, test coverage, and
operational safety. Style feedback should point to an existing contract or be
converted into one if it will matter again.
