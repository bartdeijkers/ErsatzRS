# Docker Hardware Smoke Scripts

These scripts exercise hardware paths that depend on the host runtime and
therefore cannot be promised by the published image alone.

Usage:

```bash
IMAGE=ghcr.io/bartdeijkers/ersatzrs:dev docker/smoke/nvenc.sh
IMAGE=ghcr.io/bartdeijkers/ersatzrs:dev docker/smoke/vaapi.sh
IMAGE=ghcr.io/bartdeijkers/ersatzrs:dev docker/smoke/vulkan.sh
```

NVENC requires NVIDIA Container Toolkit and Docker GPU access. VAAPI and Vulkan
require `/dev/dri` to be available to the container.
