<!--
generated: 2026-06-25
last_processed_commit: 7bf064dab02b9d058a732120fb56cf2b7a1d8020
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts, each closing **one** ErsatzTV parity gap,
ADR-backed runtime hardening gap, or **spec-backed chore** (the next slice of a
`Proposed` / `Not implemented` ADR, an active plan, or an open phase TODO).
Auto-prioritized across `docs/progress`, the ADR index, `plans/`, and
`phase.toml` per the [rules](automation/gap-prompt-rules.md); regenerated daily
by the [progress routine](automation/daily-progress-routine.md).

`cargo xtask progress gaps` is **one** input, not the whole backlog: it inspects
only whole-feature parity status and goes clear long before the specs are fully
implemented. The chores below come from the wider spec sweep (open debt,
unimplemented ADRs, active plans, and open phase TODOs) that the routine
performs.

**Parity baseline:** ErsatzTV *parity* chores are measured against ErsatzTV
commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31).
Spec-backed chores below are deliberate ErsatzRS decisions and may have no
ErsatzTV equivalent; each cites its governing ADR/plan/TODO.

Selection (2026-06-25, change window
[`dbfb60e..7bf064d`](https://github.com/bartdeijkers/ersatzrs/compare/dbfb60e...7bf064d),
two commits): `a4117e1` closed `content-variants` Phase 4 commentary render
acceptance and `7bf064d` closed Phase 5 scheduling examples, leaving the
`content-variants` feature `Ported` with every structured `[[debt]]` resolved.
`cargo xtask progress gaps` is now fully clear and **every** tracked parity
`[[debt]]` is `resolved = true` — including the prior pick-3
`ffmpeg-filter-graph` `opacity-expression-time-variable-semantics` debt, which
was resolved at `eb4dfe1` (before this window) but published stale by the
previous degraded run when the upstream pin was unreachable. That dead prompt is
removed and refilled from the ranking. Ranking: (1) **ersatzrs-healthcheck
binary** — open Phase 7 `phase.toml` `known_todo` and top `plans/INDEX.md`
deferred item (phase focus first); (2) **in-app TSDuck MPEG-TS
troubleshooting** — ADR 0013 remains Proposed / Not implemented, a
maintainer-requested diagnostic capability; (3) **ADR 0018 provider-side extras
/ alternate-version import** — the next coherent slice of the still-`Proposed`
ADR 0018, explicitly deferred in `content-variants.toml` ("Provider-specific
Plex/Jellyfin/Emby extras or version imports remain deferred"), with the pinned
ErsatzTV provider scanners as the parity reference.

Pin note (2026-06-25): the ErsatzTV `git clone` path is still blocked by the org
egress policy (`403` for `github.com/ErsatzTV/legacy`), but the pinned snapshot
was obtained via the `codeload.github.com` tarball at exactly
`0d301df5e8512fdfccf3a611188d27ab1690ee31` (the SHA is in the URL, so the
extracted tree is the commit's tree). Parity for this run was therefore measured
against the pin; the baseline was **not** advanced. The provider-import prompt
below names the exact pinned `$ERSATZTV_SRC` scanner paths to read.

---

## 1. Build the next ErsatzRS runtime hardening chore: purpose-built healthcheck binary (slug `ersatzrs-healthcheck`)

Use the repo workflow from AGENTS.md. This is a **spec-backed Phase 7 runtime
hardening chore**, not an ErsatzTV parity port: it is governed by the open
Phase 7 TODO in `docs/progress/phase.toml` and the deferred-contract entry in
`plans/INDEX.md`. It will create a new tracked slug — register
`docs/progress/parity/ersatzrs-healthcheck.toml` in the same slice. The goal is
to replace the Docker image's current external tool health check with a small
`ersatzrs-healthcheck` binary that probes ErsatzRS readiness without requiring
shell, curl, or platform-specific TCP tricks in the runtime image.

Before editing:
1. Read `Dockerfile`, `docs/progress/phase.toml`, `plans/INDEX.md`,
   `ARCHITECTURE.md`, and `BUILD.md`.
2. Inspect the existing `/healthz` route, startup health/readiness reporting,
   release archive binary layout, Docker runtime stage, and CI/release Docker
   checks.
3. Build a Source Map for health endpoint semantics, binary packaging, Docker
   `HEALTHCHECK`, release archive inclusion, and cross-platform build targets.
4. Decide whether the binary should call only `/healthz` or also expose a local
   diagnostics mode; keep the first slice narrow unless the contracts require
   more.

Implement the slice end-to-end:
- Add an `ersatzrs-healthcheck` binary in the existing workspace style with no
  secrets, no local-path assumptions, and no OS-specific behavior outside
  documented `cfg` guards.
- Have it probe the configured host/port or URL with sensible defaults for the
  Docker image (`127.0.0.1:8409/healthz`) and return standard process exit
  codes for healthy/unhealthy states.
- Update the Dockerfile to copy and use the new binary in `HEALTHCHECK`,
  keeping the runtime image compatible with Linux Docker and the existing
  managed FFmpeg runtime.
- Include the binary in release/archive packaging if the release scripts expect
  all supported service binaries to ship together.
- Update docs/progress (register the new slug), `plans/INDEX.md`, and
  `CHANGELOG.md` once the Docker health check no longer depends on external tools.

Verification:
- Focused tests for argument/default URL handling and exit-code mapping.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- A Docker build or the repo's Docker check script, proving the image contains
  and runs `ersatzrs-healthcheck`.
- At least `cargo check` or the existing platform matrix script for Linux,
  Windows, and macOS targets touched by the new binary.
- `cargo xtask progress check`.

Report the Source Map, implemented healthcheck behavior, Docker proof,
Windows/Linux/macOS compatibility proof, remaining runtime-hardening TODOs,
verification run, and changed files.

---

## 2. Build the next ErsatzRS support tooling chore: in-app TSDuck MPEG-TS troubleshooting (slug `tsduck-mpeg-troubleshooting`)

Use the repo workflow from AGENTS.md. This is a **spec-backed diagnostic
extension**, not an ErsatzTV parity port: it is governed by
[ADR 0013](adr/0013-tsduck-mpeg-troubleshooting.md) and
[`plans/mpeg-troubleshooting-tsduck.md`](../plans/mpeg-troubleshooting-tsduck.md).
The manual `scripts/e2e/tsduck-playback-gate.sh` gate exists, but it does not
satisfy the in-app Support tool. Normal IPTV, HLS, HDHomeRun, FFmpeg spawning,
startup readiness, Docker, and `/healthz` must not depend on TSDuck.

Scope v1 narrowly to channel-level MPEG-TS diagnostics at
`/system/troubleshooting/mpeg`. Do not add repair/mutation tools, replace
production streams, bundle TSDuck into Docker/release images, or implement
non-MPEG-TS container analysis in this slice.

Before editing:
1. Read ADR 0013, `plans/mpeg-troubleshooting-tsduck.md`,
   `docs/progress/phase.toml`, `plans/INDEX.md`, `ARCHITECTURE.md`, `STYLE.md`,
   and `TEST.md`.
2. Inspect the current IPTV/HLS MPEG-TS output paths, troubleshooting page
   routes, archive/sample helpers, health-check engine, language modules, and
   the existing manual TSDuck playback gate.
3. Build a Source Map for command construction, bounded sample capture,
   report parsing, archive writing, Support route rendering, health reporting,
   and redaction boundaries.
4. Identify which follow-ups remain out of scope: TSDuck packaging, raw sample
   retention by default, repair tools, deeper HLS/fMP4 analysis, teletext analysis
   and any normal playback-path dependency.

Implement the slice end-to-end:
- Add a small TSDuck diagnostic service that detects `tsp --version`, builds
  structured `tsp`/`tsbitrate` argv without a shell, and reports
  `NotAvailable` without failing startup.
- Capture bounded MPEG-TS samples for a selected channel/mode/duration, with a
  20 second default, 120 second maximum, sample-size guard, unique run id, and
  explicit raw-sample archive opt-in.
- Run read-only analysis with `continuity`, `pcrverify`, and `analyze --json`;
  parse PAT/PMT/service/PID summaries, continuity findings, PCR findings, and
  optional bitrate estimates.
- Produce a structured, redacted report and archive metadata, excluding raw
  `.ts` samples unless the user opted in.
- Add `/system/troubleshooting/mpeg`, link it from the Support Tools panel, and
  render an SSR-safe form plus latest report tables/excerpts.
- Localize every new visible label, status, help text, and error in all
  supported cultures.
- Add a non-blocking health-check row that reports TSDuck availability as
  informational or not applicable.
- Update docs/progress, plans/ADR implementation status if accepted, and
  `CHANGELOG.md` when the tool is usable.

Verification:
- Focused tests for argv construction, detection, report parsing, redaction,
  status mapping, and archive contents.
- Focused web tests for the Support link, `/system/troubleshooting/mpeg` SSR
  form, missing-TSDuck `NotAvailable`, and non-MPEG-TS `NotApplicable` states.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- A Playwright or browser proof for the troubleshooting route.
- `cargo xtask progress check`.

Report the Source Map, diagnostic behavior implemented, unsupported/missing
TSDuck behavior, archive/redaction proof, verification run, remaining
diagnostic follow-ups, and changed files.

---

## 3. Build the next ErsatzRS spec chore: ADR 0018 provider-side extras and alternate-version import (slug `content-variants`)

Use the repo workflow from AGENTS.md plus `skills/parity-slice.md`. This is the
next coherent slice of the still-`Proposed`
[ADR 0018](adr/0018-extras-alternate-versions-and-stream-roles.md): extending
content-variant handling from the local scanner to the Plex/Jellyfin/Emby
provider scanners. The `content-variants` feature is currently `Ported` for the
local-library / search / UI / playback / scheduling scope, but
`docs/progress/parity/content-variants.toml` records the explicit shortfall:
"Provider-specific Plex/Jellyfin/Emby extras or version imports remain deferred
until a provider API mapping slice is selected." The goal is fully functional
ErsatzTV parity for provider extras/alternate-version import — no stopgaps, with
any remaining shortfall recorded as visible `[[debt]]`.

This is genuine ErsatzTV parity work: do **not** invent provider behavior.
Read ErsatzTV source **at the pinned commit**
`0d301df5e8512fdfccf3a611188d27ab1690ee31` (the
[`parity-baseline`](automation/parity-baseline.md); `git clone` is blocked by
egress, so fetch via the `codeload.github.com/ErsatzTV/legacy/tar.gz/<sha>`
tarball and point `$ERSATZTV_SRC` at the extracted tree).

Before editing:
1. Read `docs/progress/parity/content-variants.toml`, ADR 0018,
   `plans/extras-alternate-versions-and-stream-roles.md`, PORTING.md, STYLE.md,
   and TEST.md.
2. Read these pinned ErsatzTV provider scanners to learn how extras /
   other-video / alternate versions are discovered and persisted:
   `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Plex/PlexMovieLibraryScanner.cs`,
   `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Plex/PlexOtherVideoLibraryScanner.cs`,
   `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Plex/PlexTelevisionLibraryScanner.cs`,
   `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Jellyfin/JellyfinMovieLibraryScanner.cs`,
   `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Jellyfin/JellyfinTelevisionLibraryScanner.cs`,
   `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Emby/EmbyMovieLibraryScanner.cs`, and
   `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Emby/EmbyTelevisionLibraryScanner.cs`.
3. Inspect the ErsatzRS local-scanner content-variant import path (the existing
   `OtherVideo` / `ContentKind=Extra` / `AlternateVersion` / `SubKind` /
   `ParentMediaItemId` handling proven by the resolved `content-variants` debts)
   so the provider path reuses the same domain shapes.
4. Build a Source Map for provider library scanning, item-kind classification,
   parent linkage, and content-variant persistence. Identify the existing
   `ParityDebt` (the provider-import deferral) you are closing or narrowing.

Implement the slice end-to-end:
- Map provider extras / other-video / alternate-version items into the same
  `ContentKind` / `SubKind` / `ParentMediaItemId` model the local scanner uses,
  preserving ErsatzTV's discovery and linkage semantics at the pin.
- Keep canonical primaries unchanged and variants hidden-by-default, matching
  the already-ported local behavior and search projection.
- Add focused scanner tests for each provider's extras/alternate-version import
  and parent linkage; do not regress existing local-scanner coverage.
- Update `docs/progress/parity/content-variants.toml`: refresh the summary,
  and if the provider work is not fully complete in this slice, flip the
  feature back to `In progress` and record the remaining provider scope as a
  structured `[[debt]]` (a `Ported` feature must not carry open debt — `cargo
  xtask progress check` fails that combination). Update `CHANGELOG.md`.
- Decide whether any deliberate deviation from ErsatzTV needs a new ADR.

Verification:
- Focused provider-scanner tests (`cargo test -p ersatzrs-scanner <filter>`)
  for extras/alternate-version classification and linkage.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- `cargo xtask progress check`.

Report the Source Map, ErsatzTV-vs-ErsatzRS provider mapping, behavior
implemented, remaining `content-variants` `ParityDebt`, verification run, and
changed files.
