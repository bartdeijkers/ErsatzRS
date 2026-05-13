# ErsatzRS

ErsatzRS is a from-scratch Rust reimplementation of
[ErsatzTV Legacy](https://github.com/ErsatzTV/legacy): a media server that turns
personal media libraries into linear TV channels served through IPTV, HLS, and
HDHomeRun-compatible endpoints.

The project is currently **pre-alpha**

## Goals

- Reach feature parity with ErsatzTV Legacy first and then fork into a 'next' version with opinionated changes.
- Use FFmpeg 8.1 as a hard runtime requirement.
- Utilize the external graphics-renderer process with FFmpeg 8.1 `drawvg`
  filters and SVG templates.
- Keep the project public-open-source ready, even while development starts as a
  solo-maintainer effort.

## Current Status

**Phase 7: Hardening and Parity Completion.** Phases 0–6 are complete. The workspace,
FFmpeg pipeline, playout scheduling, media source scanning, IPTV/HDHomeRun endpoints, and
admin UI slices through Trakt list integration are all in place. See [STATUS.md](STATUS.md)
for the current phase checklist, parity matrix, and session log.

## Quick Start

Build and run (requires FFmpeg 8.1+ on PATH):

```bash
cargo build --workspace
cargo run --bin ersatzrs-web
```

The admin UI is at http://127.0.0.1:8409. See [BUILD.md](BUILD.md) for full development
instructions, including database setup, Leptos hot-reload, and per-platform build targets.

## Key Resources

Project contracts and decisions:

- [STATUS.md](STATUS.md) — current phase, checklist, gaps, session log.
- [ARCHITECTURE.md](ARCHITECTURE.md) — crate layout, dependency rules.
- [PORTING.md](PORTING.md) — ErsatzTV porting workflow and critical files.
- [plans/INDEX.md](plans/INDEX.md) — locked decisions and deferred work.
- [CONTRIBUTING.md](CONTRIBUTING.md) — contribution workflow.
- [STYLE.md](STYLE.md) — Rust/SQL naming, errors, parity code rules.
- [TEST.md](TEST.md) — Leptos hydration rules and test playbook format.
- [docs/adr/](docs/adr/) — architectural decision records.

## Documentation

Documentation follows the Diataxis structure under [docs/](docs/README.md):

- Tutorials teach through a guided path.
- How-to guides solve specific tasks.
- Reference pages provide lookup material.
- Explanation pages describe background and tradeoffs.

## Development Requirements

- Rust stable, pinned through [rust-toolchain.toml](rust-toolchain.toml).
- FFmpeg 8.1 or newer available on `PATH` — a hard runtime requirement.
- SQLite as the default database backend (created at `~/.ersatzrs/ErsatzRS.db`).
- MySQL 8/MariaDB support behind the `mysql` Cargo feature; set
  `DATABASE_URL='mysql://user:pass@host/ersatzrs'` to use it.
- For interactive UI development: `cargo install cargo-leptos --locked` and run
  `cargo leptos build` in `crates/ersatzrs-web/` to generate WASM hydration assets.

## Credits

ErsatzRS is a Rust port of [ErsatzTV Legacy](https://github.com/ErsatzTV/legacy) by
Jason Dove. ErsatzTV is the original work this project ports from, and ErsatzRS
would not exist without it. If you find either project useful, please consider
supporting Jason's continued work on the upstream:
[paypal.me/jasongdove](https://www.paypal.me/jasongdove).

## License

ErsatzRS is licensed under the zlib license, the same license as the upstream
ErsatzTV project. See [LICENSE](LICENSE) for the full text and the preserved
upstream copyright notice.
