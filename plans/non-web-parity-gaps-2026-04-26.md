# Remaining non-web parity gaps between ErsatzTV and ErsatzRS

Last reviewed: 2026-05-02. Slice 20 closed 2026-05-02.

This plan only tracks Phase 7 non-web parity work that is still open. Historical
slice details have been removed. See `STATUS.md` and the git history when
closed-slice context is needed.

## Scope

- In scope: `ersatzrs-ffmpeg`, `ersatzrs-core`, `ersatzrs-infra`,
  `ersatzrs-scanner`, `ersatzrs-domain`, migrations, non-web documentation,
  and runtime-independent scheduler/maintenance behavior.
- Out of scope for this plan unless explicitly called out: admin UI work in
  `crates/ersatzrs-web/`. Web consumption of persisted non-web data is noted
  only where it is the remaining parity handoff.
- ErsatzTV behavior remains authoritative. Before implementing a remaining
  item, read the corresponding C# source under `/home/bart/ErsatzTV/`.

## Remaining Slice Overview

| Status | Slice | Remaining work |
| --- | --- | --- |
| Partial | Slice 16 — Health check engine | Web/admin consumption of persisted health data, real provider connectivity checks, and deeper typed hardware probes. |
| Open | Slice 21 — Phase 7 closure | Side-by-side benchmarks, 24-hour soak coverage, and final status/parity documentation. |

## Remaining Gaps

### Slice 16 — Health Check Engine

Remaining work:

1. Wire the admin health badge/page to the persisted `health.summary` and
   `health.results`, or explicitly run the health engine from the web layer
   when web scope is open again.
2. Replace the current state-based provider availability check with real
   Plex/Jellyfin/Emby connectivity polling equivalent to ErsatzTV's provider
   online/offline checks.
3. Deepen `HardwareAcceleration` and `VaapiDriver` checks once the current
   runtime capability report is expanded with per-profile/per-device probes
   comparable to ErsatzTV's `IHardwareCapabilities` factory.

Primary files:

- `crates/ersatzrs-core/src/health.rs`
- `crates/ersatzrs-infra/src/health.rs`
- `crates/ersatzrs-ffmpeg/src/version.rs`
- `crates/ersatzrs-web/` only for the final web consumption handoff

### Slice 21 — Phase 7 Closure

Phase 7 should close only after the remaining open slices are implemented and
verified. The remaining closure work is:

1. Build a side-by-side benchmark harness that runs ErsatzRS and ErsatzTV
   against identical channel configuration in Docker Compose or an equivalent
   repeatable environment.
2. Run a 24-hour single-channel soak test and record:
   - memory behavior
   - ffmpeg process restarts
   - stream gaps/skips
   - scheduler loop stability
   - health-check output

3. Verify that imported ErsatzTV databases with Classic schedules can build
   valid ErsatzRS playouts.
4. Update `STATUS.md`, `docs/reference/parity-matrix.md`, and related docs to
   mark Phase 7 complete only after the benchmark/soak evidence is available.

## Cross-Cutting Persistence Follow-Up

The remaining open persistence items that matter for the slices above are:

- Remaining media metadata joins not covered by the closed scanner/search
  slices: `Mood`, `Style`, `MusicVideoArtist`, plus any table-level read/write
  gaps still listed in `docs/reference/persistence-gap-matrix.md`.
- Polymorphic media owners still needed by scanner/search parity:
  `Image`, `ImageMetadata`, `ImageFolderDuration`, `MusicVideo`,
  `RemoteStream`, `RemoteStreamMetadata`, and `Song`.
- Stream/subtitle metadata: `Subtitle`, `LanguageCode`, and `Resolution`.
- Template/deco follow-up if later parity review shows those fields are used
  by the remaining non-web paths: `TemplateItem`, full `Deco` field
  round-trip, and read/write coverage for deco break/template rows.
- Playout overlays: `FFmpegGlobalWatermarkId` now flows through playout build
  post-processing when no item-specific watermark is set; revisit only if later
  FFmpeg rendering parity requires deeper `PlayoutItemGraphicsElement` or
  `PlayoutItemWatermark` behavior.

Keep `docs/reference/persistence-gap-matrix.md` synchronized when closing any
of these rows.

## Verification

For each implementation slice:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo insta review` when snapshots change
- `cargo deny check` when dependency or license surface changes

Phase 7 closure also requires:

- ErsatzTV and ErsatzRS run an identical channel configuration covering Block,
  YAML, Classic, and Scripted schedules, producing matching playout timing
  within the documented tolerance.
- A 24-hour soak of one IPTV channel completes without memory growth,
  restart storms, stream gaps, or scheduler instability.
- `cargo run --bin ersatzrs-scheduler` runs all maintenance loops cleanly.
- Persisted health-check results appear under `ConfigElement` keys
  `health.results` and `health.summary`.
- `cargo run --bin ersatzrs-scanner -- --provider plex --library-id N
  --deep-scan` refreshes rows, and the same command without `--deep-scan`
  skips unchanged normal-state rows on a second run.
- `/healthz` remains 200 under worker load.
