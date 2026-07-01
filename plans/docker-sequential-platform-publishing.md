# Sequential Docker Platform Publishing

> **Type:** Release stabilization plan.
> **Status:** Proposed. Can be implemented independently, but gives the biggest
> stability win when paired with `docker-prebuilt-binary-images.md`.

## Problem

The release Docker job currently asks buildx/bake to build the full multi-arch
image in one step:

- `linux/amd64`
- `linux/arm64`
- `linux/arm/v7`

When the Dockerfile performs native Rust compilation, that means multiple
expensive platform builds can contend for CPU, memory, cache, and QEMU
translation at the same time. A failure in one platform also makes the log
harder to read because the important platform-specific failure is buried inside
a combined multi-platform build.

## Goal

Build and push one platform image at a time, then create the final version and
channel manifests from the successful per-platform images.

## Non-goals

- Do not change the set of supported Docker platforms.
- Do not change GHCR image names or release tag semantics.
- Do not remove the final multi-arch manifest verification.
- Do not change release archive publication.

## Source Map

- `.github/workflows/release.yml`
  - `docker` job currently downloads `web-assets`, runs `docker/bake-action`,
    publishes a channel tag, and verifies the manifest.
- `docker-bake.hcl`
  - `target "ersatzrs-release"` inherits the shared platform list.
- `Dockerfile.release`
  - Current release Dockerfile. This plan works whether it compiles binaries
    itself or consumes prebuilt binaries from release artifacts.

## Proposed Shape

Split Docker publication into two logical stages:

1. A per-platform build matrix publishes temporary architecture tags.
2. A manifest job combines those temporary tags into the final version tag and
   channel tag.

Example temporary tags:

- `$REGISTRY_IMAGE:$VERSION-linux-amd64`
- `$REGISTRY_IMAGE:$VERSION-linux-arm64`
- `$REGISTRY_IMAGE:$VERSION-linux-arm-v7`

The final manifest step then publishes:

- `$REGISTRY_IMAGE:$VERSION`
- `$REGISTRY_IMAGE:$CHANNEL_TAG`

## Implementation Plan

### Phase 1 - Normalize Platform Metadata

- Add a Docker platform matrix with:
  - Docker platform (`linux/amd64`, `linux/arm64`, `linux/arm/v7`)
  - safe tag suffix (`linux-amd64`, `linux-arm64`, `linux-arm-v7`)
  - release RID when paired with prebuilt binaries
- Keep `fail-fast: false` so one failed platform does not cancel the others.
- Keep the existing image metadata normalization for version and channel tags.

### Phase 2 - Build Per-platform Images

- Replace the single multi-platform bake invocation with a per-platform
  build/push step.
- Either pass the platform to bake through an override file or use
  `docker buildx build --platform "$PLATFORM"`.
- Push only the temporary per-platform tag from each matrix row.
- Keep the same Dockerfile and build context behavior for each platform.

### Phase 3 - Create Manifests

- Add a final job that depends on every per-platform image row.
- Run `docker buildx imagetools create` to publish the version manifest from the
  temporary platform tags.
- Run a second `imagetools create` or equivalent tagging command to publish the
  channel tag from the version manifest.
- Keep the existing manifest inspection and platform grep checks.

### Phase 4 - Retention and Cleanup

- Decide whether temporary architecture tags are retained as useful diagnostics
  or deleted after the final manifest succeeds.
- If retaining them, document that they are implementation tags and not user
  facing release tags.

## Acceptance Checks

- GitHub Actions shows platform-specific Docker jobs with separate logs.
- A failure identifies the exact platform that failed.
- The final version manifest contains:
  - `linux/amd64`
  - `linux/arm64`
  - `linux/arm/v7`
- The channel tag points at the same image set as the version tag.
- The Docker release still uploads the manifest report artifact.
- Release archives and web-assets publication are unchanged.

## Risks and Open Questions

- Temporary tags add GHCR clutter unless cleaned up or intentionally retained.
- If a final manifest job is retried after one temporary tag was overwritten,
  it must still combine tags from the same release version.
- If this is implemented before prebuilt binary Docker images, per-platform
  builds still compile Rust, but resource contention and log clarity improve.
- If this is implemented after prebuilt binary Docker images, the per-platform
  jobs should become short packaging steps.
