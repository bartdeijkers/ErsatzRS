# Plan — HLS transcode capacity and admission control

Implements the revised [ADR 0016](../docs/adr/0016-active-hls-transcode-limit.md)
(HLS transcode capacity and admission control). This is a parity-hardening
implementation, not a workaround: read the ErsatzTV source for session lifecycle
first (porting rule), then build the capacity ceiling + admission ladder
ErsatzRS-side.

## Goal

Make `ffmpeg.segmenter.active_transcode_limit` a **truthful hard ceiling** on
concurrent transcodes while keeping start/switch latency low, and expose it in
terms an ErsatzRS admin already understands (tuners), with a live usage readout.

Two goals must hold simultaneously:

- **Viewer experience:** instant zap-back, responsive channel changes, no
  stutter on the channel the viewer settles on.
- **System load:** a real upper bound on simultaneous transcodes; the marginal
  viewer over capacity gets a graceful "all streams in use" slate instead of
  degrading every active stream.

## Current state (verified 2026-06-21)

- Reservation + admission live in
  [`crates/ersatzrs-infra/src/hls_worker.rs`](../crates/ersatzrs-infra/src/hls_worker.rs):
  `HLS_ACTIVE_TRANSCODE_COUNT`, `HLS_ACTIVE_TRANSCODE_SLOTS`,
  `try_reserve_active_transcode`, `request_active_transcode_reclaim`,
  `HLS_ACTIVE_WATCH_WINDOW = 30 s`, and the run-loop admission wiring.
  The unbounded `force_reserve_active_transcode` bypass is removed.
- `touch()` is called on every playlist poll and segment fetch
  ([`crates/ersatzrs-web/src/transport/iptv/hls.rs`](../crates/ersatzrs-web/src/transport/iptv/hls.rs)),
  so watched sessions remain inside the watch window, but touching no longer
  bypasses the configured active-transcode ceiling.
- When capacity is full, the worker now asks a lower-priority running transcode
  to release its slot in cost order (work-ahead, cooling, stale watched
  StopOnDisconnect); otherwise the cold session serves a generated
  all-streams-in-use slate without holding a slot and retries admission until it
  can switch to real content.
- Settings plumbing is complete and consistent: config key in
  [`crates/ersatzrs-domain/src/config.rs`](../crates/ersatzrs-domain/src/config.rs),
  seed default `2` in [`crates/ersatzrs-infra/src/db.rs`](../crates/ersatzrs-infra/src/db.rs),
  load/save/validate + form field + i18n (en/nl/pl/pt_br) in
  `crates/ersatzrs-web/src/admin/{pages,server_fns}/settings.rs`, troubleshooting
  summary, and `crates/ersatzrs-web/src/api/settings.rs`.
- Segmenter **rate-control is already correct** and matches the smoothness doc:
  `add_output_options` in
  [`crates/ersatzrs-ffmpeg/src/pipeline/mod.rs`](../crates/ersatzrs-ffmpeg/src/pipeline/mod.rs)
  emits `-b:v`/`-maxrate:v` + `-bufsize:v` and `-b:a`/`-maxrate:a` + `-bufsize:a`
  from the profile; `hls_output` emits a fixed 2 s closed GOP (`-g`,
  `-keyint_min == g` which disables scene-cut, `-force_key_frames` every 2 s,
  `-flags cgop`) and 4 s segments. The only genuine gap vs the doc is
  `-max_delay` for heavy 6ch AC-3 interleaving (not present here and not in
  ErsatzTV either → an ErsatzRS addition).

## Source map (read before coding — porting rule)

- ErsatzTV session lifecycle / idle / KeepRunning:
  `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/HlsSessionWorker.cs`
  `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/Commands/StartFFmpegSessionHandler.cs`
  `$ERSATZTV_SRC/ErsatzTV.Core/Interfaces/FFmpeg/IFFmpegSegmenterService.cs`
- ErsatzTV work-ahead semantics: `ffmpeg.segmenter.work_ahead_limit` usage in
  the segmenter service.
- ErsatzTV rate-control source of truth (already ported):
  `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegLibraryProcessService.cs`,
  `FFmpegProfile.VideoBufferSize` / `AudioBufferSize`.
- ErsatzRS counterparts: `hls_worker.rs` (lifecycle/reservations), pipeline
  `mod.rs` (`hls_output`, `add_output_options`).

## Workstream A — capacity ceiling + admission (ADR 0016 core)

### Stage A0 — Read + confirm
Read the ErsatzTV lifecycle sources above; confirm the ErsatzRS code map and the
exact run-loop points where a slot is acquired/held/released. No code change.

### Stage A1 — Minimum honest fix (make the number truthful)
Status: implemented 2026-06-20.

The smallest change that stops the unbounded bypass and the false labeling.

- Remove the unbounded `force_reserve_active_transcode`. Replace the run-loop
  decision so an over-limit worker either reclaims an opportunistic slot
  (work-ahead) or throttles + trims (no new FFmpeg) instead of forcing.
- Split the overloaded 30 s constant into:
  - **watch window `W`** (classification: "is this watched?") and
  - **retention window `R`** (= segmenter idle timeout: disk retention + session
    teardown). Stop holding a transcode slot for a non-pulling session; keep its
    segments on disk until `R`.
- Reword the setting label/help to the tuner model (en + nl + pl + pt_br).
- **Acceptance:** new unit test proving the counter never exceeds `N` across
  many simultaneous "watched" workers; existing reservation tests updated;
  `cargo test -p ersatzrs-infra hls`; `cargo clippy` (both passes per
  [feedback](../docs/adr/)); help-text render test updated.

### Stage A2 — Admission / preemption ladder
Status: implemented 2026-06-20 for HLS segmenter workers.

- Implement the cost-ordered ladder when a watched worker needs a slot and the
  pool is full: (1) stop a work-ahead burst, (2) reclaim a cooling
  (`W ≤ idle < R`) slot, (3) preempt the **stalest watched** session only past a
  confidence threshold, else (4) defer to Stage A3 slate.
- Track per-session last-access (already present via `last_access_ms`); the
  registry must expose enough state to find the stalest watched session.
- **Acceptance:** unit tests for each ladder branch (single zapper collapses to
  ~1 active transcode; K genuine viewers ≤ N all keep slots; N+1th defers);
  `cargo xtask smoke`; the TSDuck playback gate stays green for the watched
  channel.

### Stage A3 — Slate-on-full admission
Status: implemented 2026-06-21.

- When the ladder cannot free a slot, serve the existing offline/error slate
  (build from `ERROR_BACKGROUND_PNG` / the troubleshooting slate path) as a
  short loop that does **not** consume a real transcode slot; the worker keeps
  retrying admission and flips to real content when a slot frees.
- **Acceptance:** focused tests prove HLS waits for a capacity slate instead of
  returning the immediate 503, MPEG-TS emits slate bytes without increasing
  active/peak capacity past the occupied real slot, and the MPEG-TS response
  switches to real content after the held slot releases.

### Stage A4 — Observability + smart default
- Add a live **"Streams in use: X / N (peak Y)"** readout to the FFmpeg settings
  page and the troubleshooting summary (the counter already exists; expose a
  snapshot + peak).
- Show a hardware-aware **suggested** value next to the field from
  `ersatzrs-hwprobe` / `ersatzrs-*::health` (NVENC/QSV/VAAPI present + core
  count). Suggestion only; default stays `2`.
- Update the health-check engine if capacity now reflects a parity-sensitive
  runtime capability (per the health-check parity contract).
- **Acceptance:** `cargo xtask hydration-lint -p ersatzrs-web`;
  `cargo test -p ersatzrs-web settings`; `cargo xtask parity-check web` before
  claiming UI parity.

### Stage A5 — Pipeline-agnostic counting (follow-up)
- Fold MPEG-TS (legacy/hybrid) transcodes into the same counter so the ceiling
  means "total transcode capacity," matching the admin-facing wording. Until
  done, keep the help text scoped to HLS and record the remainder as parity
  debt.
- **Acceptance:** counter increments for MPEG-TS transcode spawns; smoke +
  TSDuck gate across all four streaming modes.

## Workstream B — segmenter smoothness + default profile (ADR 0017)

Specified by [ADR 0017](../docs/adr/0017-default-ffmpeg-profile-1080p60.md)
(five default FFmpeg profiles, 720p→4K + HDR, AC-3 6ch, HW accel auto-enabled,
bitrates tuned for 25 fps over 1 Gbps unicast). Rate-control, GOP,
scene-cut, and segment alignment are already implemented (see Current state), so
this workstream is small: seed the new default values and add one interleaving
flag.

### Stage B1 — AC-3 interleaving (`-max_delay`)
- Add `-max_delay 500000` (0.5 s) to the segmenter output for multichannel AC-3
  (ADR 0017 §2) so 6ch audio interleaves tightly with video and does not
  pop/drift at segment boundaries. Gate it to AC-3 / multichannel output.
- ErsatzRS addition (not in ErsatzTV); recorded by ADR 0017.
- **Acceptance:** `insta` argv snapshot for an AC-3 6ch segmenter command;
  TSDuck gate shows no audio-PID timestamp regressions; manual A/V-sync check at
  segment seams.

### Stage B2 — Seed the new default profile
- Update `ensure_default_ffmpeg_profile` (and its seed assertions) in
  `crates/ersatzrs-infra/src/db.rs` to the ADR 0017 §1 values: rename to
  `1920x1080 x264 ac3 6ch`, preset `veryfast`, video 15000/15000 kbit, AC-3
  640/640 kbit, 6 channels. Fresh-DB only; existing profiles are not overwritten
  (ADR 0017 §4).
- **Acceptance:** seed test asserts the new values; `cargo xtask smoke`; manual
  two-stream 1080p60 soak over the LAN stays stutter-free.

> `-muxrate` / `pkt_size` are out of scope (ADR 0017 §3): they belong to constant
> UDP push, not the `-f hls`-over-HTTP segmenter. The relevant levers are bufsize
> (done) and `-max_delay` (B1).

## Gates before "done"

- `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`
  **and** the separate `--features mysql` clippy pass (CI parity).
- `cargo test --workspace --all-features` (unit + argv snapshots via `insta`).
- `cargo xtask smoke` for runtime playback; `scripts/e2e/tsduck-playback-gate.sh
  gate` for playback correctness across MPEG-TS and HLS.
- `cargo xtask hydration-lint -p ersatzrs-web` + `cargo xtask parity-check web`
  for the settings UI.
- Update `CHANGELOG.md` (`[Unreleased]`) when each stage lands a user-visible
  change; update the parity file debt as stages resolve; record a session via
  `cargo xtask progress add-session`.

## Risks

- **Preempt-stalest heuristic** (A2) is the main behavioral risk: the idle
  threshold separating "abandoned zap" from "deep-buffered viewer" must be tuned
  against real captures, not guessed. Start conservative (prefer slate over
  preempting a borderline-active stream) and adjust with evidence.
- **Watch window `W` vs buffer depth:** if `W` is shorter than a client's HLS
  buffer, a real viewer can look idle and be wrongly demoted. Keep `W` ≥ typical
  buffer (~30 s) and validate with a real player.
- **Slate path** (A3) must not itself consume a slot or it reintroduces
  oversubscription.

## Out of scope

- Cluster-wide / multi-process scheduling (stays process-local, per ADR 0016).
- Re-deriving GOP/segment/rate-control (already correct).
- `-muxrate` CBR null-padding (UDP-push only; not the `-f hls` path).
- The unrelated `segmenter-content-timeline-deviations` work tracked separately
  in the parity file.
