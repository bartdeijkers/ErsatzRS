<!--
generated: 2026-08-07
last_processed_commit: 2ab2d2045a6f82fb59faaf1d1e923ea927bf8a1e
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: manual roadmap refresh after Phase 8-10 resequencing
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

These prompts follow the live Phase 8 focus and runnable milestone order. Phase
9 teletext work is intentionally dependency-gated behind the required Phase 8
milestones. Phase 10 video-jukebox work remains behind Phase 9 closeout and its
own ADR gate, so neither displaces runnable Phase 8 work here.

---

## 1. Implement explicit SDR and HDR10 output (slug `hdr-dolby-vision-metadata`)

Implement runnable Phase 8A milestone **p8a-01** and ADR 0064's input/output
contract. Follow AGENTS.md and
[`plans/ffmpeg-9-hdr-dolby-vision-and-pipeline-hardening.md`](../plans/ffmpeg-9-hdr-dolby-vision-and-pipeline-hardening.md).
The outcome is a truthful persisted profile choice between BT.709 SDR and
BT.2020/PQ HDR10, not a profile-name or bit-depth inference.

Before editing:

1. Read ADRs 0064, 0017, 0014, the milestone's full acceptance text, and the
   open debts in
   [`hdr-dolby-vision-metadata.toml`](progress/parity/hdr-dolby-vision-metadata.toml).
2. Trace profile persistence and validation across domain, SQLite/MySQL,
   application services, the localized profile editor, and FFmpeg argv
   generation. Map scanner side-data persistence and selected-stream flow.
3. Pin redistributable SDR, HDR10, and HLG fixtures and define decoded-frame and
   ffprobe side-data assertions before changing filters.

Implement the complete slice:

- Add the typed output-dynamic-range field, migration, repository mappings,
  localized editor choice, defaults, seeds, and invalid-combination checks.
- Preserve existing rows as SDR; seed fresh ADR 0017 HDR profiles as HDR10.
- Implement the SDR/HDR10 source-output matrix, including explicit reference
  white for SDR-to-HDR and correct post-transform metadata emission/stripping.
- Persist source mastering/content-light metadata and carry it with stream
  selection; add invalid-profile and unsupported-output health findings.
- Add SQLite/MySQL round trips, validation tests, reviewed argv snapshots,
  decoded-image comparisons, side-data assertions, and MPEG-TS/HLS transition
  proof. Do not claim Dolby Vision support in this milestone.

Close with focused crate tests, localization/hydration proof, smoke and TSDuck
for playback-sensitive changes, both Clippy configurations, test closeout,
`cargo xtask progress check`, and `git diff --check`. Update the milestone,
parity debt, changelog, prompt ranking, and session log. Do not commit or push
unless explicitly requested.

---

## 2. Harden Vulkan/libplacebo selection and fallback (slug `ffmpeg-9-performance-and-support`)

Implement runnable Phase 8B milestone **p8b-01** under ADR 0014. The goal is to
select the existing NVENC HDR Vulkan/libplacebo bridge only when its complete
operation works, then learn and surface runtime denial without making Vulkan a
mandatory platform capability or a persisted encoder kind.

Before editing:

1. Read ADRs 0014 and 0064, the milestone acceptance text, and
   [`ffmpeg-9-performance-and-support.toml`](progress/parity/ffmpeg-9-performance-and-support.toml).
2. Trace capability discovery, cache identity/invalidation, learned hardware
   denial, NVENC HDR graph construction, retry boundaries, health reporting,
   and troubleshooting serialization.
3. Record the exact functional probe graph and failure signatures for Vulkan
   device creation, libplacebo, hardware mapping, CUDA interop, required CUDA
   filters, and NVENC initialization.

Implement the complete slice:

- Introduce an operation-scoped capability keyed by FFmpeg binary, selected
  device/driver identity, and graph operation; prove it with a small real graph
  during capability refresh.
- Classify the known initialization/interoperability failures and retry exactly
  once through the next proven graph while preserving hardware encode where
  possible.
- Persist the learned denial, reuse the existing refresh invalidation lifecycle,
  and expose actionable health/troubleshooting evidence without leaking paths.
- Cover classifier, serialization, cache invalidation, primary/fallback argv,
  and the real HDR/downscale/overlay surface shape. Absence of Vulkan must remain
  a normal capability result on every supported target.

Close with focused tests, a compatible-GPU runtime gate where available,
playback smoke/TSDuck proof, both Clippy configurations, test closeout,
`cargo xtask progress check`, and `git diff --check`. Update parity, milestone,
health, changelog, prompts, and the session log. Do not add first-class Vulkan
encoding and do not commit or push unless explicitly requested.

---

## 3. Add safe FFmpeg 9 long-filtergraph file transport (slug `ffmpeg-9-performance-and-support`)

Implement runnable Phase 8C milestone **p8c-01** and ADR 0003's portable
long-command contract. Replace oversized inline complex-filter arguments with
FFmpeg 9's `-/filter_complex <path>` file-option form while preserving the CLI
spawn architecture and sanitized diagnostics.

Before editing:

1. Read ADR 0003, the milestone acceptance text, BUILD.md, testing guidance,
   and the long-filtergraph debt in
   [`ffmpeg-9-performance-and-support.toml`](progress/parity/ffmpeg-9-performance-and-support.toml).
2. Map every complex-filter construction and spawn/cancellation path. Choose one
   documented cross-platform threshold based on the complete command shape.
3. Define private temporary-file ownership, encoding, permissions where
   applicable, child-exit/cancellation cleanup, and safe diagnostics for all six
   native target shapes.

Implement the complete slice:

- Keep short graphs inline and move oversized graphs to one private per-spawn
  file referenced with structured argv; never shell-join the command.
- Guarantee deterministic cleanup after success, non-zero exit, spawn failure,
  cancellation, and dropped process supervision.
- Add snapshots and focused runtime tests for inline and forced-file paths,
  spaces and non-ASCII paths, cleanup failures, cancellation, and error output.
- Verify that logs expose the selected transport and useful sanitized context
  without dumping private media paths or full filter contents.

Close with focused pipeline tests and snapshot review, relevant cross-platform
checks, both Clippy configurations, test closeout, `cargo xtask progress check`,
and `git diff --check`. Update parity, milestone, changelog, prompt ranking, and
the session log. Do not revive removed `-filter_complex_script`; do not commit
or push unless explicitly requested.
