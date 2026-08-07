# ErsatzRS

ErsatzRS is a from-scratch Rust reimplementation of
[ErsatzTV Legacy](https://github.com/ErsatzTV/legacy): a media server that turns
personal media libraries into linear TV channels served through IPTV, HLS, and
HDHomeRun-compatible endpoints.

The project is currently in **beta** stage.

## Goals

- Reach feature parity with ErsatzTV Legacy first and then fork into a 'next' version with opinionated changes.
- Use FFmpeg 9.x as a hard runtime requirement, normally acquired from the
  first-party `ErsatzRS-ffmpeg` release at startup.
- Replace the external graphics-renderer process with FFmpeg 9 `drawvg`
  filters and generated VGS templates.
- Keep the project public-open-source ready, even while development starts as a
  solo-maintainer effort.

## Current Status

**Phase 9: Teletext support.** Phases 0–8 are complete. SDR/HDR10 output is now
an explicit profile contract, Dolby Vision input is characterized, the
Vulkan/libplacebo path is functionally gated with a proven fallback, and the
FFmpeg 9 command and media-input hardening has landed. The "The Box"
video-jukebox extension is Phase 10. See [STATUS.md](STATUS.md) for the
queryable current phase, parity, and session sources.

## Quick start

The current beta release line publishes a Windows x86-64 archive. Download and
extract `ersatzrs-<tag>-win-x64.zip`, then run `ErsatzRS.exe` from the extracted
folder. On first startup, ErsatzRS downloads and validates its matching
first-party FFmpeg runtime.

Follow [Build your first channel](docs/tutorials/getting-started.md) for the
complete local-media workflow. Normal release tags can also publish the
multi-architecture image described in
[Install with Docker](docs/how-to/install-with-docker.md). Development setup
lives in [BUILD.md](BUILD.md).

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

Documentation follows the Diátaxis structure under [docs/](docs/README.md):

- Tutorials teach through a guided path.
- How-to guides solve specific tasks.
- Reference pages provide lookup material.
- Explanation pages describe background and tradeoffs.

Common end-user guides:

- [Install on Windows](docs/how-to/install-on-windows.md)
- [Install on Linux with systemd](docs/how-to/install-on-linux.md)
- [Add and scan a local library](docs/how-to/add-and-scan-a-local-library.md)
- [Connect a Jellyfin server](docs/how-to/connect-a-jellyfin-server.md)
- [Connect an IPTV client](docs/how-to/connect-an-iptv-client.md)
- [Back up and restore ErsatzRS](docs/how-to/back-up-and-restore.md)
- [Troubleshoot startup and playback](docs/how-to/troubleshoot-startup-and-playback.md)

## Development Requirements

- Rust stable, pinned through [rust-toolchain.toml](rust-toolchain.toml).
- Network access for the first managed FFmpeg runtime download, or an explicit
  FFmpeg 9.x override/pre-placed managed runtime for offline development.
- SQLite as the default database backend (created at `~/.ersatzrs/ErsatzRS.db`).
- MySQL 8/MariaDB support behind the `mysql` Cargo feature; set
  `DATABASE_URL='mysql://user:pass@host/ersatzrs'` to use it.
- For interactive UI development: `cargo install cargo-leptos --locked`, then use
  the `./run.sh` dev entry point (`./run.sh dev` for hot-reload, `./run.sh build`
  for a CI-parity one-shot build that generates the WASM hydration assets). On
  Windows use the [`run.cmd`](run.cmd) shim. See [BUILD.md](BUILD.md) for the full
  command reference.

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
