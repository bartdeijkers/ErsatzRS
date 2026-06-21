<!--
generated: 2026-06-21
last_processed_commit: 24e8383a2155503890457543a82958248e406dba
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill - goal prompts

Actionable paste-and-run goal prompts, each closing **one** ErsatzTV parity gap
or ADR-backed parity-critical runtime hardening gap. Auto-prioritized from
`docs/progress`, with manual maintainer promotions called out inline.
Regenerated daily by the [progress routine](automation/daily-progress-routine.md);
see the [rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Current reconcile (2026-06-21): the HLS / MPEG-TS
`all-streams-in-use-playable-slate` closeout resolves the last item emitted by
`cargo xtask progress gaps`, so the progress query should report **0** remaining
features after this commit. Per `docs/automation/gap-prompt-rules.md`, completed
scope is replaced, never kept with a completed banner. This file intentionally
contains one actionable ADR-backed runtime-hardening prompt:

- ADR 0008 FFmpeg runtime acquisition from first-party
  `bartdeijkers/ErsatzRS-ffmpeg` releases.

---

## 1 - FFmpeg runtime acquisition from ErsatzRS-ffmpeg releases (ADR 0008)

Fix the next ErsatzRS parity-infrastructure gap: implement ADR 0008's startup
FFmpeg runtime acquisition manager so ErsatzRS release archives ship without
bundled FFmpeg/ffprobe, resolve an acceptable runtime at startup, download the
matching first-party `ErsatzRS-ffmpeg` package when needed, and surface update
availability on `/system/health`.

Use the repo workflow from `AGENTS.md`. This is ADR-backed runtime hardening, not
a pinned-baseline ErsatzTV behavior gap, so do not force it through
`skills/parity-slice.md`. Read
[`docs/adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md`](adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)
and [`plans/release-self-built-ffmpeg.md`](../plans/release-self-built-ffmpeg.md)
before editing. Treat ADR 0008's six supported RIDs (`linux-x64`, `linux-arm64`,
`linux-arm`, `osx-x64`, `osx-arm64`, `win-x64`) plus Docker pre-placement as
part of the compatibility contract.

Current ADR 0008 implementation state:
- Startup already detects/enforces FFmpeg 8.1 capabilities.
- The managed download/update manager, release-asset resolver, checksum
  verification, managed runtime directory, scanner ffprobe reuse, release
  workflow cleanup, Docker pre-placement, and health-page update indicator are
  not yet implemented.

Before editing:
1. Inspect `crates/ersatzrs-ffmpeg/src/version.rs`,
   `crates/ersatzrs-scanner/src/main.rs`, `ersatzrs-core::health` /
   `ersatzrs-infra::health`, `/system/health`, `.github/workflows/release.yml`,
   Docker packaging, and existing FFmpeg capability tests.
2. Build a Source Map for runtime resolution (env/config override -> managed
   runtime dir -> PATH), capability validation, download/checksum/install,
   update check, health surfacing, release artifacts, Docker images, and scanner
   ffprobe discovery.
3. Preserve the FFmpeg 8.1 + `drawvg` + `zscale` capability contract. Do not
   lower requirements, silently auto-update a working runtime, or move FFmpeg
   build ownership back into the application repo.

Implement the slice end-to-end: add the managed runtime resolver and installer,
map current OS/architecture to the six ADR RIDs, download and checksum-verify the
matching `ErsatzRS-ffmpeg` release when no valid runtime is resolved, validate
the accepted runtime with the existing capability gate plus the ADR 0008
drawvg/zscale smoke checks, record the installed tag, best-effort check latest
release at startup, expose resolved source/version/update availability through
health and localized `/system/health`, make the scanner use the same ffprobe
resolver, remove bundled-FFmpeg assumptions from release workflow/docs, and make
Docker pre-place the managed runtime when configured.

Add focused tests for: env override wins; pre-placed managed runtime bypasses
network; PATH fallback works; invalid checksum is rejected; missing runtime
downloads the RID-specific package; outdated managed runtime sets update
available without replacing it; scanner ffprobe uses the shared resolver; health
serializes resolved source/version/update status; release archive staging has no
`ffmpeg/` directory. Use test doubles for GitHub release metadata/downloads; do
not depend on live network in unit tests.

Verify with focused resolver/health/release tests, both normal and MySQL clippy
passes if shared crates change, `cargo test --workspace --all-features`,
`cargo xtask smoke`, `cargo xtask progress check`, and `cargo xtask progress
gaps`. Run `cargo xtask hydration-lint -p ersatzrs-web` if the `/system/health`
UI changes. Report the Source Map, implemented runtime behavior, remaining ADR
0008 debt if any, compatibility audit for Windows/Linux/macOS/Docker, verification
run, and changed files.
