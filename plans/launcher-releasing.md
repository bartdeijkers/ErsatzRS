# Plan: Windows tray launcher for ErsatzRS (+ graceful shutdown, win release bundle)

> **Status: Mostly shipped.** The tray launcher and graceful shutdown shipped
> (`ersatzrs-launcher`); code signing is researched but deferred (Part D).
> The Windows FFmpeg bundling parts of this plan are superseded by
> [ADR 0008](../docs/adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md):
> release archives no longer package FFmpeg/ffprobe, and startup acquires the
> managed `ErsatzRS-ffmpeg` runtime instead.

## Context

ErsatzTV ships a tiny `ErsatzTV-Windows.exe` in every Windows release. It puts an
icon in the system tray and, in the background, starts the actual server. From the
tray the user can open the admin UI in a browser, open the logs folder, or
**Exit** (which shuts the server down cleanly). The launcher exe is code-signed so
Windows SmartScreen does not warn the user.

ErsatzRS has no such launcher today: on Windows a user must run `ersatzrs-web.exe`
from a console, which leaves a console window open and offers no obvious "stop"
affordance. This task ports the launcher to ErsatzRS.

Key research finding: **ErsatzTV's launcher is itself a ~90-line Rust program**
(separate repo `ErsatzTV/ErsatzTV-Windows`, now archived at v1.0.0). It uses
`tray-item` for the tray, spawns `ErsatzTV.exe` from its own directory with
`CREATE_NO_WINDOW`, and on Exit sends Ctrl+C to the child via
`AttachConsole` + `GenerateConsoleCtrlEvent`. ErsatzRS already has every
counterpart it needs: server port `8409`, log dir `%LOCALAPPDATA%\ErsatzRS\logs`
(`ersatzrs_infra::logging::default_log_dir`), and `artwork/Ersatzrs.ico`.

### Decisions (from clarifying Q&A)

- **Signing: descoped for now.** Research is captured below and documented in
  `RELEASING.md`; CI gets a clearly-marked *optional* signing step that is skipped
  until secrets exist. Engineering effort goes into a real graceful-shutdown
  handler instead.
- **Scope: launcher crate + Windows release pipeline.** The original FFmpeg
  bundle scope is superseded by ADR 0008 startup runtime acquisition.
- **Exit behavior: graceful Ctrl+C** — add `with_graceful_shutdown` to the axum
  server and have the launcher send a console Ctrl+C to the child (one small,
  audited `unsafe` block in the launcher crate only).
- **FFmpeg for win-x64:** managed at startup through `ErsatzRS-ffmpeg` releases
  per ADR 0008.
- **Leptos assets:** include in the Windows bundle now (`cargo leptos build`).

---

## Part A — Graceful shutdown in `ersatzrs-web` (priority)

Today `crates/ersatzrs-web/src/host/bootstrap.rs:96` calls
`axum::serve(listener, app.into_make_service()).await` with **no** graceful
shutdown, and `host/tasks.rs::spawn_background_tasks` fires three fire-and-forget
`tokio::spawn`s (HDHR UDP discovery, Trakt maintenance, maintenance scheduler)
with no cancellation or join handles. We make shutdown first-class so that the
tray "Exit" (and `docker stop`, and Ctrl+C in a console) drains cleanly.

Coordinate it centrally with a `tokio_util::sync::CancellationToken`. Note:
`tokio-util` is a web-crate dep but currently only with `features = ["io"]`.

Changes:

1. **`crates/ersatzrs-web/Cargo.toml`** — add the `"signal"` feature to the
   `tokio` dependency (line 35). Ensure the `CancellationToken`/`TaskTracker`
   types are available from `tokio-util`.
2. **`host/bootstrap.rs`**:
   - Create `let shutdown = CancellationToken::new();`.
   - Add `async fn shutdown_signal(token)` that awaits the first of: `ctrl_c()`
     (fires on Windows `CTRL_C_EVENT`, the path the launcher drives), plus, under
     `#[cfg(unix)]`, a `SIGTERM` stream (so `docker stop` is clean too). On signal,
     log and `token.cancel()`.
   - Replace the serve call with
     `axum::serve(...).with_graceful_shutdown(shutdown.clone().cancelled_owned())`.
   - After serve returns: ensure `shutdown.cancel()`, await background tasks
     (see #3) with a bounded timeout, then `pool.close().await`, then flush the
     log writer. Log "shutdown complete".
3. **`host/tasks.rs`** — thread the `CancellationToken` into
   `spawn_background_tasks`, collect the `JoinHandle`s (or use a
   `tokio_util::task::TaskTracker`), and make each loop cancellation-aware:
   - `run_trakt_maintenance`: `select!` the `interval.tick()` against
     `token.cancelled()`.
   - `run_udp_discovery` (in `transport/hdhr`): same treatment around its recv loop.
   - `run_maintenance_scheduler`: pass the token into `SchedulerOptions` and have
     `ersatzrs-scheduler::runner::run_scheduler` select on it **instead of** its
     own internal `ctrl_c()` when a token is supplied (extend `SchedulerOptions`
     with an optional `shutdown: Option<CancellationToken>`; keep the standalone
     `ctrl_c` fallback as a safety default — note `run_scheduler` is currently
     only called from the web host).

This part is independently valuable and testable without the launcher.

---

## Part B — New crate `crates/ersatzrs-launcher` (Windows tray app)

A faithful port of `ErsatzTV-Windows`, modernized to maintained crates and made a
no-op on non-Windows so `cargo build --workspace` stays green on the Linux
self-hosted CI. This is an intentional platform exception (documented per the
AGENTS.md platform-parity rule, analogous to the Unix-only-test carve-out).

**`crates/ersatzrs-launcher/Cargo.toml`**
- New workspace member (add to `Cargo.toml:members`).
- `[lints]` block that **omits** `workspace = true` and sets
  `rust.unsafe_code = "allow"` with a comment — the workspace forbids unsafe, but
  this crate needs the Win32 console Ctrl+C call. Scope the actual `unsafe` to one
  small function.
- `[[bin]] name = "ersatzrs-launcher"` (packaged into the zip as `ErsatzRS.exe`).
- `[target.'cfg(windows)'.dependencies]`:
  - `tray-item = "0.10"` — mirrors ErsatzTV's proven structure almost line-for-line.
  - `open = "5"` — cross-platform "open URL / open folder".
  - `directories = "6"` — match the version `ersatzrs-infra` uses; resolve
    `data_local_dir()/ErsatzRS/logs` consistently with `logging::default_log_dir`.
  - `windows = { version = "<current>", features = ["Win32_System_Console",
    "Win32_Foundation"] }` — for the Exit Ctrl+C.
  - `process_path = "0.1"` — locate the launcher's own exe dir.
- `[target.'cfg(windows)'.build-dependencies]`: `embed-resource` (or `winres`) to
  compile a `.rc` that embeds `artwork/Ersatzrs.ico` under a named icon resource
  plus version info; `static_vcruntime = "2"` to statically link the CRT.

**`crates/ersatzrs-launcher/ersatzrs_launcher.rc`** — defines
`ersatzrs-icon ICON "../../artwork/Ersatzrs.ico"` (+ `VERSIONINFO`).

**`crates/ersatzrs-launcher/build.rs`** — `#[cfg(windows)]` compile the `.rc` and
call `static_vcruntime::metabuild()`; empty otherwise.

**`crates/ersatzrs-launcher/src/main.rs`**
- `#![cfg_attr(windows, windows_subsystem = "windows")]` (no console window).
- `#[cfg(not(windows))] fn main()` — print "ErsatzRS launcher is Windows-only" and
  exit 0 (keeps cross-platform `cargo build --workspace` working).
- `#[cfg(windows)] fn main()` ports the ErsatzTV logic:
  1. Build tray `TrayItem::new("ErsatzRS", "ersatzrs-icon")`.
  2. Menu **"Open Web UI"** → `open::that("http://localhost:8409")`.
  3. Menu **"Show Logs"** → `create_dir_all` + `open::that` on
     `data_local_dir()/ErsatzRS/logs`.
  4. separator, **"Exit"** → send `Message::Exit` over an mpsc channel.
  5. Locate `ersatzrs-web.exe` next to the launcher (`process_path`), and spawn it
     with `CREATE_NO_WINDOW`, `stdin/out/err = null`. Set
     `ERSATZRS_FFMPEG__PATH` / `ERSATZRS_FFPROBE__PATH` on the child to the
     co-located `ffmpeg.exe`/`ffprobe.exe` when present, plus `LEPTOS_SITE_ROOT`
     to the bundled `site` dir so the interactive UI loads.
  6. On `Exit`: the one `unsafe` block — `AttachConsole(child_pid)` then
     `GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0)`; then `child.wait()`. The child's
     `shutdown_signal` (Part A) turns this into a clean drain.

---

## Part C — Windows release pipeline

Edit the `win-x64` matrix entry / steps in **`.github/workflows/release.yml`**
(currently only copies `ersatzrs-web.exe` + `ersatzrs-scanner.exe`). Note:
`ffbuild/` is empty and no RID currently builds Leptos assets in `release.yml`
(only `docker/Dockerfile` does), so both must be added from scratch.

1. Build the launcher too: add `--bin ersatzrs-launcher` to the win-x64 build.
2. Build Leptos assets (win-x64): `rustup target add wasm32-unknown-unknown`,
   `cargo install cargo-leptos --locked`, then
   `(cd crates/ersatzrs-web && cargo leptos build --release)` → `target/site`.
3. Do not bundle FFmpeg/ffprobe. The release workflow guards the staged archive
   against `ffmpeg*` / `ffprobe*` entries; startup downloads the managed runtime.
4. Stage layout: `ErsatzRS.exe` (launcher) + `ersatzrs-web.exe` +
   `ersatzrs-scanner.exe` + `site/` (Leptos assets) + existing `assets/` +
   LICENSE + README.
5. Zip as `ersatzrs-<version>-win-x64.zip` and upload to the release.

---

## Part D — Code signing (researched; implementation deferred)

Documented in `RELEASING.md`; CI gets an **optional, skipped-by-default** step.

Findings (June 2026):
- **Azure Artifact Signing** (formerly *Trusted Signing*) is what ErsatzTV uses, via
  `azure/trusted-signing-action@v0` with OIDC Azure login. ~**$9.99/mo**. For Public
  Trust certs, **individuals are eligible only in the US/Canada**; an EU individual
  must enroll as an **organization**. SmartScreen reputation **builds over download
  history — not instant.**
- **OV cert + Azure Key Vault HSM** (~$200-400/yr) works for individuals; reputation
  also builds over time.
- **EV cert** clears SmartScreen *instantly*, but requires a registered org +
  hardware token (~$300-700/yr).

CI hook to add (disabled until secrets present), gating on
`if: ${{ secrets.AZURE_CLIENT_ID != '' }}`, before the zip step:
```yaml
- name: Sign Windows binaries (optional)
  if: ${{ secrets.AZURE_CLIENT_ID != '' }}
  uses: azure/trusted-signing-action@v0
  with:
    endpoint: https://eus.codesigning.azure.net/
    trusted-signing-account-name: <account>
    certificate-profile-name: ErsatzRS
    files-folder: ${{ github.workspace }}/<stage>
    files-folder-filter: ErsatzRS.exe,ersatzrs-web.exe,ersatzrs-scanner.exe
    file-digest: SHA256
    timestamp-rfc3161: http://timestamp.acs.microsoft.com
    timestamp-digest: SHA256
```
Plus the `azure/login@v2` OIDC step and `secrets: AZURE_CLIENT_ID/TENANT_ID/
SUBSCRIPTION_ID`. Document the one-time setup in `RELEASING.md`.

---

## Files to create / modify

Create:
- `crates/ersatzrs-launcher/{Cargo.toml, build.rs, ersatzrs_launcher.rc, src/main.rs}`

Modify:
- `Cargo.toml` — add `crates/ersatzrs-launcher` to `members`.
- `crates/ersatzrs-web/Cargo.toml` — add `signal` to tokio features.
- `crates/ersatzrs-web/src/host/bootstrap.rs` — graceful shutdown + cleanup.
- `crates/ersatzrs-web/src/host/tasks.rs` — cancellation-aware tasks + token plumbing.
- `crates/ersatzrs-web/src/transport/hdhr/udp.rs` — cancellation in UDP discovery loop.
- `crates/ersatzrs-scheduler/src/runner.rs` + `Cargo.toml` — accept optional
  shutdown token.
- `.github/workflows/release.yml` — launcher build, Leptos assets, no-FFmpeg
  archive guard, optional signing.
- `RELEASING.md` — Windows bundle layout + signing setup.
- AGENTS.md / platform-parity note — record the intentional Windows-only crate.
- Session log: `cargo xtask progress add-session ...` per the handoff ritual.

## Verification

- **Build/lint (cross-platform sanity):** `cargo build --workspace` and the
  CI-matching clippy passes (default + `--features mysql`) on Linux.
- **Graceful shutdown (Linux, no Windows needed):** `cargo run --bin ersatzrs-web`,
  hit `curl http://127.0.0.1:8409/healthz`, then send `Ctrl+C` / `SIGTERM`; confirm
  logs show the shutdown sequence, background tasks stop, the DB pool closes, and
  the process exits 0 within the timeout. Add a test that `shutdown_signal` + token
  cancels background loops.
- **Windows (manual, on a Windows host or VM):** build the win-x64 zip, unzip,
  run `ErsatzRS.exe`; verify tray icon, "Open Web UI", "Show Logs", managed
  FFmpeg download/update, interactive UI, and clean "Exit".
- **Release dry-run:** run the workflow on a branch and confirm the win-x64 zip
  contains launcher + web + scanner + site + assets, excludes ffmpeg/ffprobe, and
  the optional sign step is skipped without secrets.

## Risks / follow-ups

- **FFmpeg 8.1 win64 with `drawvg`:** managed at startup via ADR 0008; startup
  validation asserts drawvg/zscale before accepting the runtime.
- **Leptos site assets in the win bundle** must be produced (cargo-leptos); launcher
  sets the matching `LEPTOS_SITE_ROOT` env.
- **`unsafe` carve-out:** the launcher crate opts out of the workspace
  `unsafe_code = "forbid"` lint for exactly one console-control call; keep it
  isolated and commented.
- **Auto-start on login** is intentionally out of scope.
