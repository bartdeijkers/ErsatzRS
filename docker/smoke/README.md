# Docker Hardware Smoke Scripts

These scripts exercise hardware paths that depend on the host runtime and
therefore cannot be promised by the published image alone. The Docker image
does not package FFmpeg/ffprobe; each script uses the managed
`ErsatzRS-ffmpeg` runtime inside the container, downloading and verifying the
checksum for `ERSATZRS_FFMPEG_RELEASE_TAG` (default `v2.0.1`) when the runtime is
not already present under `/data/.ersatzrs/ffmpeg/current`.

Usage:

```bash
IMAGE=ghcr.io/bartdeijkers/ersatzrs:dev docker/smoke/nvenc.sh
IMAGE=ghcr.io/bartdeijkers/ersatzrs:dev docker/smoke/nvenc-vulkan.sh
IMAGE=ghcr.io/bartdeijkers/ersatzrs:dev docker/smoke/vaapi.sh
IMAGE=ghcr.io/bartdeijkers/ersatzrs:dev docker/smoke/vulkan.sh
```

NVENC requires NVIDIA Container Toolkit and Docker GPU access. The
`nvenc-vulkan.sh` check also requires Vulkan driver support on that same NVIDIA
device; it derives Vulkan from CUDA and cannot pass using a software Vulkan
device. The images request NVIDIA compute, video, graphics and utility
capabilities. `vaapi.sh` and `vulkan.sh` use `/dev/dri` for Intel/AMD devices.
Host driver/device support is still required; an advertised FFmpeg feature or
a passing software Vulkan check does not prove NVIDIA interop.
