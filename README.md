# ErsatzRS

ErsatzRS is a from-scratch Rust reimplementation of
[ErsatzTV](https://github.com/ErsatzTV/ErsatzTV): a media server that turns
personal media libraries into linear TV channels served through IPTV, HLS, and
HDHomeRun-compatible endpoints.

The project is currently **pre-alpha**. It is in phase 0 bootstrap: contracts,
workspace layout, tooling, and architecture are being established before the
first Rust crates are added.

## Goals

- Reach long-term feature parity with ErsatzTV.
- Ship as a simpler deployment: one Rust server binary plus a scanner binary.
- Use FFmpeg 8.1 as a hard runtime requirement.
- Replace the external graphics-renderer process with FFmpeg 8.1 `drawvg`
  filters and SVG templates.
- Keep the project public-open-source ready, even while development starts as a
  solo-maintainer effort.

## Current Status

There is no `Cargo.toml` yet and no runnable application. Until the workspace is
created, the repository is intentionally focused on project contracts:

- [Contributing](CONTRIBUTING.md)
- [Code style](STYLE.md)
- [Architecture](ARCHITECTURE.md)
- [Porting contract](PORTING.md)
- [Testing contract](docs/how-to/testing.md)
- [Documentation contract](docs/CONTRIBUTING.md)
- [ADRs](docs/adr/)
- [Project contracts and status](plans/INDEX.md)
- [Current phase status](STATUS.md)

## Documentation

Documentation follows the Diataxis structure under [docs/](docs/README.md):

- Tutorials teach through a guided path.
- How-to guides solve specific tasks.
- Reference pages provide lookup material.
- Explanation pages describe background and tradeoffs.

## Development Requirements

The full developer setup will be introduced with the Cargo workspace. The
long-term baseline is:

- Rust stable, pinned through [rust-toolchain.toml](rust-toolchain.toml).
- FFmpeg 8.1 or newer available on `PATH`.
- SQLite as the default database backend.
- MySQL 8/MariaDB-compatible backend support behind the `mysql` Cargo feature;
  use a `mysql://...` or `mariadb://...` `DATABASE_URL` when building with it.

## Credits

ErsatzRS is a Rust port of [ErsatzTV](https://github.com/ErsatzTV/ErsatzTV) by
Jason Dove. ErsatzTV is the original work this project ports from, and ErsatzRS
would not exist without it. If you find either project useful, please consider
supporting Jason's continued work on the upstream:
[paypal.me/jasongdove](https://www.paypal.me/jasongdove).

## License

ErsatzRS is licensed under the zlib license, the same license as the upstream
ErsatzTV project. See [LICENSE](LICENSE) for the full text and the preserved
upstream copyright notice.
