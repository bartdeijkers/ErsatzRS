# ADR 0008: FFmpeg 8.1 runtime and multi-arch images

## Status

Proposed

## Context

ErsatzRS requires FFmpeg 8.1 for the `drawvg` filter and uses `ffprobe` for
media scanning. The original Phase 0 Docker image was enough for early local
validation, but Phase 7 needs explicit startup checks and a publishable
multi-architecture image strategy.

ErsatzTV used a separate `ErsatzTV-ffmpeg` repository derived from
`jrottenberg/ffmpeg` and `linuxserver/docker-ffmpeg`. ErsatzRS keeps packaging
inside this repository so the application binary, migrations, admin UI assets,
FFmpeg build, and runtime checks evolve together.

FFmpeg 8.1 `drawvg` executes VGS scripts and requires FFmpeg to be configured
with `--enable-cairo`. Earlier ErsatzRS graphics code and docs describe SVG
templates feeding `drawvg`; that compatibility gap needs a separate graphics
follow-up and must not be hidden by the container work.

## Decision

Own FFmpeg and ffprobe packaging in this repository. Use `docker/Dockerfile`
and `docker-bake.hcl` to build one redistributable ErsatzRS image for:

- `linux/amd64`
- `linux/arm64`
- `linux/arm/v7`

Published images must not enable `--enable-nonfree`, bundle the CUDA toolkit,
include libnpp, or rely on non-redistributable FFmpeg components. The Docker
build fetches the official FFmpeg 8.1 source tarball and verifies this SHA-256:

```text
b072aed6871998cce9b36e7774033105ca29e33632be5b6347f3206898e0756a
```

Startup checks must validate both FFmpeg and ffprobe are version `>= 8.1`.
The check must also collect filters, encoders, decoders, hardware accelerators,
and configure flags from the local FFmpeg binary. Core CPU/drawvg capabilities
are required everywhere; GPU capabilities are detected and advertised from the
actual runtime image instead of assumed from the CPU architecture.

The default support matrix is:

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
flags fail startup before the web host binds.

The admin FFmpeg profile editor uses the captured startup capability report
instead of spawning `ffmpeg -hwaccels` on each request. Hardware acceleration
options shown in the UI reflect what the current runtime actually reports.

The default image is broader and slower to build than a CPU-only image, but it
keeps deployment simple and avoids a separate FFmpeg image repository. Nonfree
CUDA experiments may be added only as local, unpublished build targets unless a
future ADR accepts the licensing and distribution impact.

The `drawvg` SVG/VGS mismatch remains intentionally visible. Container support
does not make existing SVG templates valid VGS scripts.

## Alternatives Considered

- Separate FFmpeg image repository: rejected because it splits application and
  runtime compatibility decisions across repositories.
- Third-party FFmpeg base image: rejected because ErsatzRS needs FFmpeg 8.1
  drawvg and a controlled redistributable capability set.
- Variant image tags for CPU/GPU/CUDA: rejected for this slice because they
  multiply release and support paths before the base multi-arch image is stable.
- Published nonfree CUDA image: rejected because redistributability must remain
  clear for the default project image.
- Dropping `linux/arm/v7`: rejected for this slice; it remains CPU-only.

## References

- [Project contracts and status](../../plans/INDEX.md)
- [Current phase status](../../STATUS.md)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0005: Use drawvg for in-stream graphics](0005-drawvg-graphics-engine.md)
- [ErsatzTV-ffmpeg](https://github.com/ErsatzTV/ersatztv-ffmpeg)
- [jrottenberg/ffmpeg](https://github.com/jrottenberg/ffmpeg)
- [linuxserver/docker-ffmpeg](https://github.com/linuxserver/docker-ffmpeg)
- [Docker multi-platform builds](https://docs.docker.com/build/building/multi-platform/)
- [NVIDIA Container Toolkit Docker configuration](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/docker-specialized.html)
- [FFmpeg drawvg filter documentation](https://www.ffmpeg.org/ffmpeg-filters.html#drawvg)
