# Future implementation plans

The authoritative work queue is [structured progress](../docs/progress/README.md):
use `cargo xtask progress next` and `cargo xtask progress milestones --runnable`.
Milestone TOMLs own acceptance, evidence, blockers and delivery history; parity
TOMLs own feature status and debt. Decisions belong in ADRs.

Keep a standalone plan only for future implementation detail that does not fit
comfortably in its TOML record. Every milestone status may omit `plan`; when
present the link must resolve to an existing file here. Transfer unique history
and evidence before pruning a completed or superseded plan. Do not infer
acceptance from implementation notes or from the presence of a release.

## Current corrective and acceptance work

- [Automatic add-on publication](automatic-addon-publication.md) — AMM-P-01.
- [Windows candidate hardening](windows-beta-release-hardening.md) — remaining package and release acceptance.

## Future feature and engineering detail

These plans include proposals and deferred work, not blanket implementation approval.

- [Local replacement workflow](addon-list-replacement-ux.md) — discovery, suggestions and reverse assignment; bulk matching remains a product choice.
- [Teletext](teletext-support.md) — Phase 9, starting with the product/standards ADR.
- [Video jukebox](application-layer-and-video-jukebox.md) — Phase 10, after teletext.
- [Channel packages](channel-packages.md) — Phase 11 draft; product/feed ADR first.
- [Upstream catch-up](upstream-catch-up.md) — latest-upstream ledger and per-commit procedure; original triage remains in [the seed plan](ErsatzRS_catchup.md).
- [Hardware residue](phase-8-hardware-residue.md) — deferred hardware-dependent proof and Vulkan decision.
- [Prebuilt Docker images](docker-prebuilt-binary-images.md) — assemble release images from native artifacts.
- [Persistent Docker cache](docker-persistent-build-cache.md) — retain BuildKit work between attempts.
- [Sequential platform publishing](docker-sequential-platform-publishing.md) — per-platform images followed by manifest assembly.
- [Web server/WASM split](web-server-wasm-crate-split.md) — separate native server and hydration build roles.

## Navigation

Reusable design material lives in the
[UI reference](../docs/reference/ui/README.md), not in this directory.
The [contribution contract](../CONTRIBUTING.md),
[architecture](../ARCHITECTURE.md), [ADRs](../docs/adr/README.md), and
[documentation contribution guide](../docs/CONTRIBUTING.md) own project policy.
