# Resume Prompt - HLS Transcode Capacity Closeout

Created because the machine was about to shut down. Use this prompt to resume
the interrupted gap #1 work after reboot:

```text
Continue the active goal: implement gap fix #1 from docs/top-three-gaps-to-fill-prompts.md, verify CI/Windows/Linux/macOS/Docker compatibility, and commit. Work from current state; do not restart from scratch.

Before editing, read AGENTS.md plus the relevant repo contracts for the touched
areas. Cargo commands must respect the configured target-dir in
.cargo/config.toml, which currently points at /mnt/f/ErsatzRS/target and may
require elevated execution in the sandbox.

Current gap #1 is `hls-variant-playlists-segmenter`, ADR 0016 active-transcode capacity follow-up. The open debts were:
- `active-transcode-observability-and-suggestion`
- `pipeline-agnostic-transcode-capacity-counting`

Current implementation state as of 2026-06-21T01:05:02+02:00:
- Added `crates/ersatzrs-infra/src/transcode_capacity.rs` with shared process-local active transcode reservations, active/limit/peak snapshotting, HLS reclaim support, and tests.
- Updated `crates/ersatzrs-infra/src/hls_worker.rs` to use the shared capacity module instead of its private HLS-only slot map.
- Updated MPEG-TS direct/legacy/hybrid stream spawns in `crates/ersatzrs-web/src/transport/iptv/stream.rs` to reserve the same capacity slots and release them on process exit or response-body drop.
- Added settings-page active/limit/peak readout and hardware/software suggested limit in `crates/ersatzrs-web/src/admin/server_fns/settings.rs` and `crates/ersatzrs-web/src/admin/pages/settings.rs`.
- Added the same capacity object to troubleshooting JSON in `crates/ersatzrs-web/src/admin/server_fns/troubleshooting.rs`.
- Updated admin i18n strings in en-US, nl, pl, and pt-BR. The label now says streaming transcodes, not HLS-only transcodes.

Verification already run and passed:
- `cargo fmt --all`
- `cargo test -p ersatzrs-infra transcode_capacity`
- `cargo test -p ersatzrs-infra active_transcode`
- `cargo test -p ersatzrs-web settings`

Important dirty-tree notes:
- `.cargo/config.toml` is dirty only because it adds `target-dir = "/mnt/f/ErsatzRS/target"`; preserve it and do not stage it unless the maintainer explicitly wants that.
- `docs/README.md` is dirty and `docs/explanation/streaming-reliability-sampling.md` is untracked; these were already present during wrap-up and were not part of the ADR 0016 implementation. Inspect before touching or staging.
- `plans/resume-hls-transcode-capacity-closeout.md` is this handoff prompt. Keep it for resumption, but do not include it in the final implementation commit unless the maintainer explicitly asks for that.
- The intended implementation files currently include:
  - `crates/ersatzrs-infra/src/transcode_capacity.rs`
  - `crates/ersatzrs-infra/src/hls_worker.rs`
  - `crates/ersatzrs-infra/src/lib.rs`
  - `crates/ersatzrs-web/src/transport/iptv/stream.rs`
  - `crates/ersatzrs-web/src/admin/server_fns/settings.rs`
  - `crates/ersatzrs-web/src/admin/pages/settings.rs`
  - `crates/ersatzrs-web/src/admin/server_fns/troubleshooting.rs`
  - `crates/ersatzrs-web/src/admin/i18n.rs`
  - `crates/ersatzrs-web/src/admin/languages/en_us.rs`
  - `crates/ersatzrs-web/src/admin/languages/nl.rs`
  - `crates/ersatzrs-web/src/admin/languages/pl.rs`
  - `crates/ersatzrs-web/src/admin/languages/pt_br.rs`

Next steps:
1. Confirm the dirty tree with `git status --short --branch`, then inspect the current diff carefully, especially the shared `transcode_capacity` module and the `spawn_stream_body` reservation lifetime.
2. Run focused stream tests:
   - `cargo test -p ersatzrs-web spawn_stream_body_releases_mpegts_capacity`
   - `cargo test -p ersatzrs-web troubleshooting`
3. Check one behavior detail before docs: troubleshooting JSON currently embeds `ActiveTranscodeCapacityStatus`; decide whether snake_case output is acceptable there or whether a PascalCase troubleshooting wrapper is needed for API-shape consistency.
4. If focused tests pass, update docs/status:
   - `docs/adr/0016-active-hls-transcode-limit.md`
   - `plans/hls-transcode-capacity-and-admission.md`
   - `docs/progress/parity/hls-variant-playlists-segmenter.toml`
   - `CHANGELOG.md`
   - `docs/top-three-gaps-to-fill-prompts.md` only after `cargo xtask progress gaps` proves the ranking changed
   - `docs/progress/sessions.ndjson` via `cargo xtask progress add-session --agent "Codex" --summary "<one line>"`
5. Run required gates, using the configured target-dir from `.cargo/config.toml`:
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo clippy --workspace --all-targets --features mysql -- -D warnings`
   - `cargo test --workspace --all-features`
   - `cargo xtask hydration-lint -p ersatzrs-web`
   - `cargo xtask smoke`
   - `cargo xtask progress check`
   - `cargo xtask progress gaps`
   - Run `scripts/e2e/tsduck-playback-gate.sh gate` if the MPEG-TS reservation lifetime is judged runtime-sensitive enough for manual playback proof.
6. Compatibility review before commit:
   - Confirm no new `#[cfg]` gaps or hard-coded OS/arch assumptions.
   - Confirm the MPEG-TS shell-command tests remain cross-platform (`cmd.exe /C` on Windows, `/bin/sh -c` on Unix).
   - Confirm Docker compatibility: no host device probing in the request path; the suggestion uses the already-cached FFmpeg runtime capability report.
7. Stage only the intended files, not `.cargo/config.toml`, unrelated docs, or this resume prompt unless inspection proves they belong.
8. Commit after gates pass, likely:
   - `fix(streaming): expose transcode capacity across pipelines`

Do not mark the active goal complete until the implementation, docs/status updates, CI-equivalent checks, compatibility review, and commit are all done.
```
