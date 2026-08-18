# Phase 8 hardware residue plan

## Status

Actionable, **blocked on hardware that the primary development machine does not
have**. Written 2026-08-08 as a hand-off so this work can proceed independently
of Phase 9 (teletext) and Phase 10 (video jukebox).

Phase 8 itself is complete: all seven required milestones (`p8a-01`, `p8a-02`,
`p8b-01`, `p8b-02`, `p8c-01`, `p8c-02`, `p8c-03`) shipped. The first follow-up
review's eleven defects, its fourteen residue debts, and the second review's six
stream-supervisor and graph-state corrections are closed. What remains is five
parity debts that could not be closed on the available hardware, plus the
deferred milestone `p8c-04`.

Nothing here blocks Phase 9. Every item is additive hardening or an
evidence-gated decision.

## Who this is for

An agent or contributor with access to hardware the primary machine lacks:
Intel (QSV), AMD (VAAPI / AMF), Apple Silicon (VideoToolbox), or Rockchip
(RKMPP) — or to the NVIDIA/Vulkan environment needed for the remaining Dolby
Vision Profile 5 production-path acceptance.

Read `AGENTS.md` (`CODEX.md` is a symlink to it) first, then `CONTRIBUTING.md`,
`STYLE.md`, and `PORTING.md`. The rules that matter most for this work:

- **Never invent hardware evidence.** If the machine cannot produce a
  measurement, record the missing gate instead of guessing. This whole plan
  exists because that rule was followed.
- **Decoded-output evidence beats generated argv.** Every Phase 8 claim was
  proven by probing the decoded stream, not by asserting a command line. Keep
  that standard: an `insta` argv snapshot is a regression gate, not proof that
  a graph runs.
- Preserve existing user changes; never reset or overwrite a dirty worktree.
- Conventional Commits, one reviewable commit per debt, and stage only the
  intended files.
- Mark a debt resolved only when its evidence is real and recorded in
  `docs/progress/parity/<slug>.toml`.

## Why these five are still open

Measured on the primary development machine on 2026-08-08. `ffmpeg -hwaccels`
advertises `cuda`, `vaapi`, `qsv`, `drm`, `opencl`, `vulkan`, and `amf`, and
`-encoders` lists `hevc_vaapi`, `hevc_qsv`, and `hevc_amf`. Only `cuda` can
actually create a device:

| Probe | Result |
|---|---|
| `-init_hw_device cuda` | works; `hevc_nvenc` encodes |
| `-init_hw_device vaapi` | `Device creation failed: -542398533` |
| `-init_hw_device qsv` | `Device creation failed: -542398533` |
| `-init_hw_device opencl` | `Failed to get number of OpenCL platforms: -1001` |
| `hevc_qsv` | `Error creating a MFX session: -9` |
| `hevc_amf` | `DLL libamfrt64.so.1 failed to open` |
| `/dev/dri` | does not exist |

Vulkan resolves to `llvmpipe (software)`, so any Vulkan measurement taken there
is a software-rasterizer artefact rather than a verdict on a GPU.

This is itself the *advertised-but-nonfunctional* failure class that ADR 0014's
functional graph probe was built to catch, so it confirms the design — while
leaving every non-NVIDIA graph unproven.

## The five debts

Query live state with `cargo xtask progress gaps`, and a single debt with
`cargo xtask progress parity --feature <slug> --full`.

Feature slugs: **HDR** = `hdr-dolby-vision-metadata`, **PERF** =
`ffmpeg-9-performance-and-support`.

| Debt id | Feature | Needs |
|---|---|---|
| `hdr10-is-uncharacterized-on-non-nvidia-hardware` | HDR | Intel / AMD / Apple / Rockchip device |
| `hardware-graph-probes-cover-only-the-nvidia-graphs` | PERF | same |
| `hardware-hdr-candidates-need-real-gpu-measurement` | PERF | Vulkan GPU, VAAPI device, AMD device |
| `dolby-vision-playback-is-not-fixture-proven` | HDR | Matching NVIDIA/Vulkan hardware for the Profile 5 production-path gate |
| `first-class-vulkan-encoding-is-undecided` | PERF | multi-vendor evidence, then an ADR |

---

## Workstream A — Characterize HDR10 on non-NVIDIA hardware

Closes `hdr10-is-uncharacterized-on-non-nvidia-hardware`. This is the highest
value item: today an HDR10 profile on QSV, VAAPI, AMF, VideoToolbox, RKMPP, or
V4L2M2M silently demotes to software encoding. Correct, but slow, and the
health check has to warn about it.

### Contract to preserve

ADR 0064 (`docs/adr/0064-hdr-and-dolby-vision-input-output-contract.md`). An
HDR10 profile must produce a stable BT.2020/PQ stream for every source dynamic
range in the schedule:

- **PQ source** — preserve. Do not inject `-mastering_display` /
  `-content_light`; the source's own side data survives the encode and is more
  accurate than any default.
- **HLG source** — normalize to PQ.
- **SDR source** — map in at ITU-R BT.2408 diffuse white, 203 cd/m², without
  inventing highlight detail. The decoded 10-bit luma for 100% white must be
  **573**.
- Signalling must not change at a programme boundary.
- Dolby Vision side data must be stripped from HDR10 output; all HDR side data
  must be stripped from SDR output.

### Steps for a new backend

1. Reproduce the NVIDIA characterization on the new hardware. Follow
   `docs/explanation/hdr-output-characterization.md` — every command there is
   self-contained and generates its fixtures from `lavfi`.
2. Build the backend's HDR10 graph. Two constraints the NVENC work discovered
   the hard way, and which will bite on other backends too:
   - **`hwdownload` can only produce the surface's own software format.** An
     8-bit source in a hardware surface downloads as `nv12`; asking for
     `p010le` fails. A 10-bit source is the reverse. Derive the format from the
     *source* pixel format, never from the profile's desired output depth. See
     `hardware_surface_download_format` in
     `crates/ersatzrs-ffmpeg/src/filter/hdr.rs`.
   - **`zscale` cannot convert from an unknown colourspace** — an untagged
     source fails with `code 3074: no path between colorspaces`. Untagged files
     are common, so assert input tags first with a BT.709 fallback.
3. Verify from decoded frames, not argv. Minimum bar, matching what NVENC had
   to clear:
   - all three source types produce `bt2020nc / smpte2084 / bt2020` with
     mastering display and MaxCLL/MaxFALL present;
   - the three outputs concatenated into one MPEG-TS report a **single**
     signalling class across all frames;
   - the graph survives an alpha overlay, burned subtitles, an input seek, and
     an HLS run.
4. Flip the backend in `hdr10_output_support`
   (`crates/ersatzrs-ffmpeg/src/filter/hdr.rs:233`) from `Unproven` to
   `Proven`. **Update these in the same commit** or the health report will lie:
   - `HDR10_PROVEN_ACCELERATIONS` in
     `crates/ersatzrs-infra/src/health/checks/output_dynamic_range.rs` (its doc
     comment says to keep it in step);
   - the demotion branch at `crates/ersatzrs-ffmpeg/src/pipeline/mod.rs:2452`;
   - the changelog entry that currently names the demoted backends.
5. If a backend genuinely **cannot** signal HDR10, that is also a result worth
   committing. Record it as `Unproven` with the measurement that shows why, and
   the health check keeps warning honestly.

### Acceptance for Workstream A

Decoded-frame evidence for at least one non-NVIDIA backend, recorded as a new
finding in `docs/explanation/hdr-output-characterization.md`, with the parity debt
either resolved (all backends settled) or narrowed with the remaining vendors
named.

---

## Workstream B — Extend the functional graph probe

Closes `hardware-graph-probes-cover-only-the-nvidia-graphs`. Pairs naturally
with Workstream A: characterize the graph, then teach the probe to verify it at
runtime.

### Why the probe exists (ADR 0014)

Advertised capability is not runnable capability. The production bug that
motivated this: `-hwaccels` listed `vulkan`, so the pipeline selected the
Vulkan/libplacebo HDR graph — but the only Vulkan device was llvmpipe, and
every stream died with `Could not derive CUDA device`. The fix was to stop
trusting the capability list and run a tiny real graph instead.

The evidence rule is **positive proof, not absence of denial**: a graph is
selectable only when a probe has proven it on the exact device in use. See
`prove_hardware_graph` / `hardware_graph_proven` in
`crates/ersatzrs-ffmpeg/src/version.rs`.

### Steps for a new probe

1. Add a variant to `HardwareGraphOperation`
   (`crates/ersatzrs-ffmpeg/src/graph_probe.rs:32`). Each variant needs a
   stable `id()` string — it is persisted — and a matching arm in `from_id`.
2. Add its probe arguments. Keep them minimal and fast; note that NVENC
   required at least 256x144, so respect each encoder's minimum dimensions.
3. Extend `GraphFailureKind` if the new backend fails in a way the existing
   classifier does not recognise. Classify from **captured real stderr**, not
   from guessed message text.
4. Lift the `cuda` gate in `applicable_graph_operations`
   (`crates/ersatzrs-infra/src/hardware_graph.rs:285`), which currently returns
   `None` unless `cuda` is advertised. Only probe graphs whose prerequisites
   are advertised: a platform without a backend must stay a normal capability
   result, never a denial.
5. Check the persistence cache key still holds.
   `PersistedHardwareGraphState::is_reusable_for` keys on the ffmpeg path and
   the device identity; confirm the new backend's device string is a real
   identity and not an opaque blob. (`nvidia_gpu_list` was originally a raw
   ffmpeg blob, which made both the health message and the cache key useless —
   hence `hardware_graph_device()`.)

### Acceptance for Workstream B

A probe that proves the graph on working hardware **and** denies it on hardware
where the graph is advertised but non-functional. Both directions matter; the
denial direction is the whole point. Retry-path coverage in
`crates/ersatzrs-ffmpeg/tests/` for the new operation.

---

## Workstream C — Measure the deferred HDR benchmark candidates

Closes `hardware-hdr-candidates-need-real-gpu-measurement`. Three of the four
`p8b-02` candidates were unmeasurable.

Harness: `scripts/bench/hdr-graph-bench.sh`, run as

```bash
ERSATZRS_FFMPEG=/path/to/ffmpeg scripts/bench/hdr-graph-bench.sh both
```

It reports device availability up front and prints `UNAVAILABLE` for a
candidate it cannot run, so an empty comparison is never mistaken for a tie.

| Candidate | Needs | Prior reading |
|---|---|---|
| Fused libplacebo tone-map + scale | Vulkan GPU | 12.03s vs 3.95s on llvmpipe |
| libplacebo peak detection | Vulkan GPU | unmeasured |
| Native `tonemap_vaapi` vs OpenCL bridge | VAAPI device | unmeasured |
| AMF colour conversion | AMD hardware | unmeasured |

The llvmpipe reading is a software-rasterizer artefact, not a verdict on the
graph.

Adopt a candidate only when it both **preserves ADR 0064 output semantics** and
shows a **measured win on real hardware**. Peak detection deserves particular
scrutiny: it is content-adaptive, so it trades the reproducibility of the
static-metadata path for per-scene adaptation. That is a product decision, not
only a speed one — a channel whose tone-mapping shifts between programmes is a
regression even if it benchmarks faster.

Record measurements in the parity debt whether or not anything is adopted. A
measured "no win" closes the debt just as validly as a measured win.

---

## Workstream D — Dolby Vision Profile 5 production acceptance

Closes `dolby-vision-playback-is-not-fixture-proven`.

Profiles 8.1 and 7 are implemented from characterized conventional base layers.
Profile 7 direct software decode is limited to the exact dual-layer tuple in
ADR 0064, and `dovi_split=mode=bl` remains a characterization oracle rather
than a production dependency.

Profile 5 now has exact SDR/H.264 and HDR10/HEVC operations using the selected
CUDA device, RPU-aware Vulkan/libplacebo processing, software colour-contract
conversion, and NVENC. Functional probes, health, command selection, HLS/IPTV
refusal, and learned-denial behavior are covered. A public Profile 5 sample was
characterized through equivalent graphs, but the complete production HLS and
MPEG-TS paths still need a run on matching NVIDIA/Vulkan hardware.

### Remaining gate

1. Run the public Profile 5 sample through the real web host's HLS and direct
   MPEG-TS paths with both SDR and HDR10 profiles on matching hardware.
2. Confirm positive operation-scoped capability proof selects the intended
   graph and a forced failure denies it without a guessed-colour fallback.
3. Decode and compare representative output frames, verify SDR/HDR10
   signalling, confirm Dolby Vision metadata is absent, and run the transport
   checks required by the playback gate.
4. Re-run the six supported target/runtime build and packaging gates before
   describing Profile 5 support as portable; unsupported targets must retain
   their explicit refusal rather than selecting an unproven graph.
5. Record only redacted stream characteristics and results. Do not commit the
   media, provider URLs, or maintainer-machine paths.
6. Close the parity debt only when the production-path evidence is attached to
   `docs/progress/parity/hdr-dolby-vision-metadata.toml`.

---

## Workstream E — The first-class Vulkan encoding decision (`p8c-04`)

Closes `first-class-vulkan-encoding-is-undecided`. This is a **decision**
milestone, deliberately deferred, and it is gated on Workstreams B and C.

Current state: ErsatzRS's production Vulkan use is an internal libplacebo
*processing* bridge for NVENC, not an encoder backend. Native Windows/NVIDIA
measurements proved that FFmpeg's `h264_vulkan` and `hevc_vulkan` encoders can
produce structurally valid HLS and MPEG-TS, but the result is one-vendor,
synthetic-content evidence with codec-specific trade-offs. It is not enough to
justify a persisted acceleration kind.

**Do not add a persisted Vulkan hardware-acceleration kind before the ADR
exists.** No domain enum variant, no migration, no profile UI, no encoder
priority change. The evidence required first:

- multi-vendor quality and latency comparison;
- device selection behavior across vendors;
- MPEG-TS / HLS mux and client compatibility;
- fallback behavior when the graph fails;
- packaging across all six release RIDs (see the platform table in
  `AGENTS.md`).

If the evidence justifies it, write a separate ADR and get it accepted before
any schema, UI, or priority change. Otherwise retain the deferral **with the
measurements attached** — a documented "not yet, and here is why" is a complete
outcome for this milestone.

---

## Suggested order

Workstreams A and B pair per vendor: characterize the graph, then teach the
probe to verify it. C follows on the same hardware while it is available. D is
independent and gated on matching hardware. E is gated on B and C.

Doing one vendor end-to-end (A + B + C) is far more valuable than doing
Workstream A across several vendors, because the probe is what keeps the
characterization honest at runtime.

## Verification

Per-change inner loop:

```bash
./run.sh check -p <crate>
./run.sh test -p <crate> <filter>
```

Closeout gate — run once, not per edit:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
./run.sh test-closeout
cargo xtask progress check
cargo xtask architecture-lint
cargo xtask smoke
```

For playback-correctness changes also run the manual TSDuck gate, which is not
part of CI:

```bash
scripts/e2e/tsduck-playback-gate.sh gate
```

Two clippy passes are required and **neither** uses `--all-features`: that
mirrors CI, and without the separate `--features mysql` pass the feature-gated
`#![cfg(not(feature = "mysql"))]` test files escape lint entirely.

`target/` grows fast. Prune with `cargo sweep --time 30`; run `cargo clean`
once it passes 40 GiB.

## Recording the outcome

For each debt closed or narrowed:

1. Edit the `[[debt]]` entry in `docs/progress/parity/<slug>.toml`. Add a
   `remediation` field with the evidence and set `resolved = true`, or extend
   `evidence` with what was measured and what still blocks it. See the
   `update-parity` skill (`skills/update-parity.md`).
2. Add a finding to `docs/explanation/hdr-output-characterization.md` for anything
   HDR-related, with reproducible commands.
3. Update `CHANGELOG.md` under `[Unreleased]` for anything user-visible — a
   backend gaining hardware HDR10 certainly is.
4. `cargo xtask progress check`, then
   `cargo xtask progress add-session --agent "<name>" --summary "<one line>"`.
5. Update `docs/progress/phase.toml` if the set of blocked items changes; its
   `next_steps` currently lists all five with their missing gates.

## Reference

- `docs/progress/milestones.toml` — the completed Phase 8 milestones
  `p8a-01` through `p8c-03`, their acceptance boundaries, and evidence. Query
  with `cargo xtask progress milestones --id <id> --full`.
- `docs/adr/0064-hdr-and-dolby-vision-input-output-contract.md` — the output
  contract.
- `docs/adr/0014-runtime-learned-hardware-capability-fallback.md` — why probes
  exist and the positive-proof rule.
- `docs/adr/0003-ffmpeg-cli-spawn.md` — FFmpeg stays a `tokio::process::Command`
  CLI spawn. **Do not add libav bindings.**
- `docs/explanation/hdr-output-characterization.md` — eleven findings with
  reproducible commands; the model for new evidence.
- `docs/how-to/troubleshoot-startup-and-playback.md` — includes the measured
  rotation/display-matrix table from the closed rotation debt.
