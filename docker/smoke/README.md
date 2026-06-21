# Docker Hardware Smoke Scripts

These scripts exercise hardware paths that depend on the host runtime and
therefore cannot be promised by the published image alone. The Docker image
does not package FFmpeg/ffprobe; each script uses the managed
`ErsatzRS-ffmpeg` runtime inside the container, downloading and verifying the
checksum for `ERSATZRS_FFMPEG_RELEASE_TAG` (default `v1.0.1`) when the runtime is
not already present under `/data/.ersatzrs/ffmpeg/current`.

Usage:

```bash
IMAGE=ghcr.io/bartdeijkers/ersatzrs:dev docker/smoke/nvenc.sh
IMAGE=ghcr.io/bartdeijkers/ersatzrs:dev docker/smoke/vaapi.sh
IMAGE=ghcr.io/bartdeijkers/ersatzrs:dev docker/smoke/vulkan.sh
```

NVENC requires NVIDIA Container Toolkit and Docker GPU access. VAAPI and Vulkan
require `/dev/dri` to be available to the container.
