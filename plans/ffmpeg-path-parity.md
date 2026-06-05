# FFmpeg/ffprobe invocation: investigation + locator parity

## Context

Two suspicions raised by the maintainer:

1. The **Windows release build** does not recognize `ffmpeg.exe` / `ffprobe.exe`
   when they sit in the same folder as `ErsatzRS.exe`.
2. The **other builds** (Linux/macOS) also do not follow best practice when
   invoking ffmpeg/ffprobe.

Task: investigate how ffmpeg/ffprobe are actually invoked in a release build (do
not run a build). The findings below confirm **both** suspicions and trace them
to a single missing mechanism: ErsatzRS has no equivalent of ErsatzTV's
`FFmpegLocator`.

Chosen scope: **full parity** with `FFmpegLocator` (shared resolver: DB config →
next-to-executable → PATH → persist), replacing the env-only logic everywhere.

## Findings — how it works today

Path resolution lives entirely in `crates/ersatzrs-ffmpeg/src/version.rs`:

- `ffmpeg_path_from_env()` / `ffprobe_path_from_env()` (lines 325–341) read
  **only** a set of env vars (`ErsatzRS_FFMPEG__PATH`, `ERSATZRS_FFMPEG__PATH`,
  `FFMPEG_PATH`, and the ffprobe equivalents). When absent they fall back to the
  **bare string** `"ffmpeg"` / `"ffprobe"` (`DEFAULT_FFMPEG_PATH`, lines 9–10).
- The bare `"ffmpeg"` is handed straight to `Command::new(...)` and therefore
  relies entirely on OS `PATH` resolution. No `.exe` suffix is added and the
  directory of the running executable is never searched explicitly.

Every spawn site uses these helpers (none read any other path source):
- `crates/ersatzrs-ffmpeg/src/command.rs:14` — `FfmpegCommand::new` sets
  `executable: ffmpeg_path_from_env()`. That `executable` is spawned in
  `crates/ersatzrs-infra/src/hls_worker.rs:296,1191,1333` and
  `crates/ersatzrs-web/src/transport/dev_stream.rs:174,336`.
- Direct calls: `hls_worker.rs:1152/1163/1216/1244`,
  `transport/iptv/stream.rs:758/817/881/915`, `subtitles.rs:190/539`,
  `playback.rs:641`, `server_fns/watermarks.rs:521`, `server_fns/libraries.rs:181`.
- The scanner has its **own duplicate** `ffprobe_path_from_env()` in
  `crates/ersatzrs-scanner/src/main.rs:398` (same env-only logic).

Startup check (`crates/ersatzrs-web/src/host/bootstrap.rs:280`) calls
`check_ffmpeg_runtime()`, which also uses `FfmpegRuntimeCheckConfig::from_env()`
(version.rs:158) → the same env-only paths.

### Suspicion 1 confirmed (Windows, next to the .exe)
ErsatzRS passes a bare `"ffmpeg"` and never explicitly searches the directory of
the running executable, nor adds `.exe`. Resolution then depends on Rust's
`Command` search behavior, which does not reliably include the directory of the
process's own executable — only `PATH`. If `ffmpeg.exe` sits next to
`ErsatzRS.exe` but is not on `PATH`, it is not found. ErsatzTV solves this
explicitly (see below).

### Suspicion 2 confirmed (not best practice)
- The admin settings page **does store** `CONFIG_KEY_FFMPEG_PATH` /
  `CONFIG_KEY_FFPROBE_PATH` in the DB (`server_fns/settings.rs:435–446`) and
  validates them (`validate_tool_path`, settings.rs:1008), **but no spawn site
  reads that DB value**. The configured path is purely cosmetic — it is ignored
  at execution time.
- The resolved path is never persisted; the logic is duplicated (scanner).
- `AppState` already holds `ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>`
  (`host/state.rs:181`) with `report.ffmpeg.path` / `report.ffprobe.path`, but
  those values come from the env-only check and are ignored by spawn sites.

### Parity reference: ErsatzTV `FFmpegLocator`
`$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegLocator.cs` does it correctly:
1. Read DB config for the key; if the file exists → use it. If it no longer
   exists → delete the config and re-locate.
2. Locate: **first** next to the running executable
   (`Path.GetDirectoryName(Environment.ProcessPath)` + `.exe` on Windows),
   **then** via `which` (Unix) / `where` (Windows) on `PATH`.
3. Persist the resolved absolute path back into DB config.

## Recommended remediation (parity with `FFmpegLocator`)

Introduce one shared resolver and route all spawn sites and the startup check
through it. Suggested placement: `crates/ersatzrs-ffmpeg/src/version.rs` (or a
new `locator.rs` in the same crate), so `ersatzrs-infra`, `ersatzrs-web` and
`ersatzrs-scanner` all share it.

1. **New resolution function** `resolve_ffmpeg_path` / `resolve_ffprobe_path`
   trying, in order:
   a. explicit env var (keep existing `*_PATH` vars for override/CI);
   b. DB config value supplied by the caller (if the file exists);
   c. next to the running executable via `std::env::current_exe()` + `.parent()`,
      with `.exe` suffix under `#[cfg(windows)]`;
   d. on `PATH` (e.g. the `which` crate, or `where`/`which`, or scanning `PATH`
      entries — platform-neutral, per the platform-parity rule);
   e. fall back to the bare `"ffmpeg"` / `"ffprobe"`.
   Return the resolved path so the caller can persist it to DB config (step (b)
   of ErsatzTV).

2. **DB wiring at startup**: in `bootstrap.rs`, inject the path resolved from DB
   (`CONFIG_KEY_FFMPEG_PATH` / `CONFIG_KEY_FFPROBE_PATH`) into the
   `FfmpegRuntimeCheckConfig` (new constructor next to `from_env`), and write the
   eventually-located absolute path back to DB config when it was empty/invalid —
   mirroring `FFmpegLocator.LocateExecutableAsync`.

3. **Spawn sites use the resolver.** Lowest-churn route: have `AppState` carry the
   resolved absolute paths (derived from the existing `ffmpeg_runtime` report) and
   pass them to the spawn sites, instead of the loose `ffmpeg_path_from_env()`
   calls. For `command.rs` (`FfmpegCommand::new`), add a variant accepting an
   explicit path instead of calling `ffmpeg_path_from_env()` itself.

4. **Remove the scanner duplicate** (`scanner/src/main.rs:398`) and use the shared
   resolver; the scanner can keep its `--ffprobe-path` arg as an explicit override
   (step a).

5. **Health-check parity**: `ersatzrs-infra/src/health.rs` uses the same
   resolution so the health report reflects the real path (per the Health-Check
   Parity rule in CLAUDE.md).

6. **Localization**: if any settings UI text changes (e.g. a hint that the path may
   be left empty for auto-detection next to the executable), update translations
   in `crates/ersatzrs-web/src/admin/languages/` (en_us, nl, pl, pt_br) per
   ADR 0012.

## Critical files

- `crates/ersatzrs-ffmpeg/src/version.rs` — path resolution + `FfmpegRuntimeCheckConfig`.
- `crates/ersatzrs-ffmpeg/src/command.rs` — `FfmpegCommand::new` executable source.
- `crates/ersatzrs-web/src/host/bootstrap.rs` — startup check + DB inject/persist.
- `crates/ersatzrs-web/src/host/state.rs` — `AppState` carrying resolved paths.
- `crates/ersatzrs-web/src/admin/server_fns/settings.rs` — path storage/validation.
- `crates/ersatzrs-scanner/src/main.rs` — remove duplicate.
- Spawn sites: `hls_worker.rs`, `transport/iptv/stream.rs`, `transport/dev_stream.rs`,
  `subtitles.rs`, `playback.rs`, `server_fns/watermarks.rs`, `server_fns/libraries.rs`.
- Reference: `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegLocator.cs`.

## Verification

- **Unit**: tests for the resolver — env override wins; DB path used when file
  exists, ignored when not; next-to-exe found (tempdir with a fake script next to
  a fake "current_exe"); `.exe` suffix under Windows cfg; PATH fallback. Mirror
  the existing pattern in `version.rs` `unix_tests`.
- **Parity**: compare behavior 1:1 with `FFmpegLocator.cs` (DB → next-to-exe →
  PATH → persist).
- **Lint/test**: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` (plus the separate
  `--features mysql` pass), `cargo test --workspace`.
- **Manual (Windows)**: place `ffmpeg.exe`/`ffprobe.exe` next to `ErsatzRS.exe`,
  start with no env vars and empty DB config; expect the startup log to show the
  absolute path next to the exe and the path to be written into DB config.
- **Manual (Linux/macOS)**: empty DB path + ffmpeg on PATH → resolved and
  persisted; then a configured DB path → used at spawn time.

## Parallel work alongside Codex

Codex is concurrently working in the same working tree on `/system/troubleshooting`
(uncommitted changes in `pages/system/troubleshooting.rs`,
`server_fns/troubleshooting.rs`, `server_fns/mod.rs`, `routes.rs`, `i18n.rs`,
`languages/{en_us,nl,pl,pt_br}.rs`, `docs/progress/parity/support-pages.toml`).

Overlap analysis against this plan:
- **No overlap** on the core: `version.rs`, `command.rs`, `bootstrap.rs`,
  `state.rs`, `settings.rs`, `scanner/main.rs`, `health.rs` and all spawn sites are
  untouched by Codex.
- **Only conflict risk**: `languages/{en_us,nl,pl,pt_br}.rs` (step 6). Codex edits
  these too.

Strategy to avoid collisions:
1. **Work in an isolated git worktree** on its own branch (e.g.
   `feat/ffmpeg-locator`). Codex's uncommitted changes in the main tree stay
   untouched and do not mix into these commits. `.cargo/config.toml` (target-dir)
   is checked in and applies in the worktree too; cargo's build lock handles
   concurrent builds.
2. **Touch the language files as late and as minimally as possible.** The FFmpeg
   settings page already exists with translation keys; only add a key if genuinely
   new UI text is needed (auto-detection hint), as a separate final commit so any
   merge with Codex's language changes stays trivial and isolated. When in doubt,
   defer until Codex has committed the troubleshooting work.
3. **Rebase/merge** the worktree branch onto `main` once Codex's troubleshooting
   commit lands; resolve only the small language-file conflicts.

## Out of scope (unless requested)
Do not run an actual release build (as requested). The pixel-format/HW path
parity debt remains a separate task.
