# ADR 0008: FFmpeg 8.1 runtime and multi-arch images

## Status

Accepted 2026-06-21

## Implementation

Implemented — ErsatzRS release archives and Docker images ship without
FFmpeg/ffprobe. Startup resolves explicit overrides first, then validates the
managed runtime, auto-updates it from `ErsatzRS-ffmpeg` releases when a newer
release is available, downloads a managed runtime when none is usable, and only
then falls back to `PATH` if managed acquisition fails or is disabled. Downloaded
archives are checksum-verified, staged, validated with the normal FFmpeg 8.1
capability gate plus drawvg/zscale smoke transcodes, and promoted only after
validation succeeds. `/system/health` and a startup admin toast report successful
downloads/updates and failed update attempts. First-party FFmpeg builds are
produced by the separate `ErsatzRS-ffmpeg` repository.

## Revision (2026-06-19): dedicated build repo + download-on-startup runtime

This ADR originally (a) kept the FFmpeg/ffprobe build inside the ErsatzRS
application repository and (b) bundled the built binaries into every release
archive / container image. Both sub-decisions are revised:

1. **Build location.** A first-party build repository now exists at
   [`bartdeijkers/ErsatzRS-ffmpeg`](https://github.com/bartdeijkers/ErsatzRS-ffmpeg)
   (a fork of [`ErsatzTV/ErsatzTV-ffmpeg`](https://github.com/ErsatzTV/ErsatzTV-ffmpeg)).
   **All first-party FFmpeg 8.1 build effort — configure flags, multi-RID
   compilation, redistributable artifact/image production, and FFmpeg build CI —
   lives there**, and that repository publishes per-RID FFmpeg/ffprobe packages as
   GitHub Releases.

2. **Acquisition.** ErsatzRS application artifacts ship **without** bundled
   FFmpeg/ffprobe. A startup runtime manager resolves, downloads, and update-checks
   FFmpeg against the published `ErsatzRS-ffmpeg` releases (see Decision below).

The Context, Decision, Consequences, and Alternatives sections below are written
against this revised model. The capability matrix and redistributability
constraints are unchanged — they now constrain what the published `ErsatzRS-ffmpeg`
packages must satisfy.

## Context

ErsatzRS requires FFmpeg 8.1 for the `drawvg` filter and uses `ffprobe` for
media scanning. The original Phase 0 Docker image was enough for early local
validation, but Phase 7 needs explicit startup checks and a publishable
multi-architecture image strategy.

ErsatzTV used a separate `ErsatzTV-ffmpeg` repository derived from
`jrottenberg/ffmpeg` and `linuxserver/docker-ffmpeg`, and required operators to
supply a compatible FFmpeg. ErsatzRS originally kept the build inside this
repository and bundled it into release archives. Bundling six per-RID payloads
into every ErsatzRS release is heavy and couples the FFmpeg lifecycle to the
application release cadence: a new FFmpeg build cannot reach users without
re-cutting an ErsatzRS release. Decoupling the two — letting FFmpeg ship and
update on its own cadence from `bartdeijkers/ErsatzRS-ffmpeg` — keeps ErsatzRS
artifacts small and lets the runtime be refreshed independently.

FFmpeg 8.1 `drawvg` executes VGS scripts and requires FFmpeg to be configured
with `--enable-cairo`. ADR 0005 owns the graphics-engine rendering decision:
drawvg receives generated VGS scripts, while SVG assets are source/reference
material only.

## Decision

**Build ownership.** First-party FFmpeg/ffprobe builds are produced by the
separate [`bartdeijkers/ErsatzRS-ffmpeg`](https://github.com/bartdeijkers/ErsatzRS-ffmpeg)
repository and published as GitHub Releases with one redistributable
FFmpeg+ffprobe package per supported RID (`linux-x64`, `linux-arm64`,
`linux-arm`, `osx-x64`, `osx-arm64`, `win-x64`). Published packages must not
enable `--enable-nonfree`, bundle the CUDA toolkit, include libnpp, or rely on
non-redistributable FFmpeg components. That repository remains the source of
truth for the official FFmpeg 8.1 source pin and its SHA-256.

**Application packaging.** ErsatzRS application artifacts (release archives and
container images) ship **without** FFmpeg/ffprobe.

**Startup runtime manager.** On startup ErsatzRS resolves an FFmpeg/ffprobe
runtime in this order:

1. Explicit environment/config override (an operator-provided ffmpeg/ffprobe
   path) — always wins and is never auto-updated.
2. Managed runtime directory under the ErsatzRS data dir (a previously
   downloaded `ErsatzRS-ffmpeg` package), auto-updated at startup when a newer
   release is available.
3. Managed download from the latest `ErsatzRS-ffmpeg` release, or an
   operator-pinned release tag, when no valid managed runtime exists.
4. `PATH` (`ffmpeg` / `ffprobe`), for developer installs and download-failure
   fallback.

Downloaded packages must match the current RID, verify against the published
checksum, extract into a staged directory, pass the full capability/version
contract below, and only then promote into the managed runtime directory and
record the installed release tag. A failed update attempt keeps the previous
valid managed runtime and reports a warning. A failed first install falls back to
`PATH` when possible; otherwise startup fails with an explicit error.
Offline/air-gapped deployments satisfy the runtime via the env override,
pre-placed managed directory, or disabled downloads plus `PATH`.

**Update + notification.** The manager records the installed
`ErsatzRS-ffmpeg` release tag and, at startup, queries the latest published
release unless an operator pinned a release tag. A missing runtime is
auto-downloaded; an outdated managed runtime is auto-downloaded and promoted
before the web host starts. A successful download/update and a failed
update/download attempt are surfaced through the health report, `/system/health`,
and a startup admin toast. A running runtime is never swapped mid-operation.
This adds an FFmpeg-runtime status (resolved source, installed version,
download/update state, update failure) to the health-check parity surface in
`ersatzrs-core::health` / `ersatzrs-infra::health`.

**Capability enforcement (unchanged).** Startup checks must validate both FFmpeg
and ffprobe are version `>= 8.1` and collect filters, encoders, decoders,
hardware accelerators, and configure flags from the resolved FFmpeg binary. Core
CPU/drawvg capabilities (including `--enable-cairo`/`drawvg` and
`--enable-libzimg`/`zscale`) are required everywhere; GPU capabilities are
detected and advertised from the actual resolved runtime instead of assumed from
the CPU architecture.

**Containers.** ErsatzRS container images do not package FFmpeg/ffprobe. Their
data volume contains the managed runtime directory, so the first start downloads
the matching `ErsatzRS-ffmpeg` release into persistent state and later starts
reuse or update it. Operators may pre-place a managed runtime in that volume for
offline deployments. `ErsatzRS-ffmpeg` owns any multi-arch FFmpeg image
production.

The default support matrix that published `ErsatzRS-ffmpeg` packages must satisfy
is:

| Platform | Required support | GPU support target |
| --- | --- | --- |
| `linux/amd64` | CPU transcode, ffprobe, drawvg | NVENC/NVDEC, VAAPI, Vulkan, QSV when host devices/runtime are provided |
| `linux/arm64` | CPU transcode, ffprobe, drawvg | VAAPI/Vulkan where redistributable packages compile; NVENC only when the build exposes it |
| `linux/arm/v7` | CPU transcode, ffprobe, drawvg | None promised |

Host GPU access remains an operator responsibility. NVIDIA requires NVIDIA
Container Toolkit or equivalent `--gpus` configuration. VAAPI and Vulkan require
the relevant `/dev/dri` devices to be passed through.

## Consequences

Runtime failure becomes explicit: missing or old FFmpeg/ffprobe binaries,
missing `drawvg`, missing core encoders/decoders, or missing required configure
flags fail startup before the web host binds. When no runtime is resolved, the
manager attempts a managed download before failing.

ErsatzRS release artifacts shrink because they no longer carry six per-RID FFmpeg
payloads, and the FFmpeg runtime can be updated independently of the application
release cadence: publishing a new `ErsatzRS-ffmpeg` release makes managed
installations download it on next startup without re-releasing ErsatzRS.

First run gains a network dependency. The manager must handle offline/air-gapped
deployments gracefully with a clear error and a documented manual-install path
(env override or pre-placed managed directory), and must verify the published
checksum of every downloaded package before use. ErsatzRS pins a minimum/known-good
`ErsatzRS-ffmpeg` release so the capability contract is met even before any
update.

The admin FFmpeg profile editor uses the captured startup capability report
instead of spawning `ffmpeg -hwaccels` on each request. Hardware acceleration
options shown in the UI reflect what the current resolved runtime actually
reports. The `/system/health` page additionally reports the resolved FFmpeg
source, installed version, latest version, successful downloads/updates, and
failed update attempts.

Nonfree CUDA experiments may be produced only as local, unpublished
`ErsatzRS-ffmpeg` build targets unless a future ADR accepts the licensing and
distribution impact. The managed runtime can execute drawvg; this does not change
the graphics-engine contract that generated drawvg artifacts are VGS scripts.

## Alternatives Considered

- Bundle FFmpeg into every ErsatzRS release archive/image (the prior decision):
  superseded — it bloats artifacts with six per-RID payloads and couples FFmpeg
  updates to ErsatzRS releases. Replaced by startup download/update from
  `ErsatzRS-ffmpeg` releases.
- Build FFmpeg inside the application repository: superseded — moved to the
  dedicated `bartdeijkers/ErsatzRS-ffmpeg` repository so the FFmpeg build matrix
  does not live in the application repo; the application repo keeps runtime
  detection, the download/update manager, and health reporting that preserve the
  application↔runtime compatibility link.
- Rely solely on a user/system FFmpeg: rejected — ErsatzTV's lesson is that public
  builds can miss `drawvg`/`zscale` or emit version/configuration output the
  runtime checker rejects; the env-override/PATH paths remain for operators who
  knowingly supply their own.
- Mid-operation FFmpeg runtime swaps: rejected — updates happen only during
  startup, before playback/transcode workers are spawned, and failures keep the
  previous valid runtime.
- Third-party FFmpeg base image: rejected because ErsatzRS needs FFmpeg 8.1
  `drawvg` and a controlled redistributable capability set.
- Published nonfree CUDA package: rejected because redistributability must remain
  clear for the default project artifacts.
- Dropping `linux/arm/v7`: rejected; it remains a CPU-only target.

## References

- [Project contracts and status](../../plans/INDEX.md)
- [Current phase status](../../STATUS.md)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0005: Use drawvg for in-stream graphics](0005-drawvg-graphics-engine.md)
- [ErsatzRS-ffmpeg (first-party build repository)](https://github.com/bartdeijkers/ErsatzRS-ffmpeg)
- [ErsatzRS-ffmpeg releases (runtime download/update source)](https://github.com/bartdeijkers/ErsatzRS-ffmpeg/releases)
- [ErsatzTV-ffmpeg (upstream)](https://github.com/ErsatzTV/ersatztv-ffmpeg)
- [jrottenberg/ffmpeg](https://github.com/jrottenberg/ffmpeg)
- [linuxserver/docker-ffmpeg](https://github.com/linuxserver/docker-ffmpeg)
- [Docker multi-platform builds](https://docs.docker.com/build/building/multi-platform/)
- [NVIDIA Container Toolkit Docker configuration](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/docker-specialized.html)
- [FFmpeg drawvg filter documentation](https://www.ffmpeg.org/ffmpeg-filters.html#drawvg)
