# Docker Images from Prebuilt Release Binaries

> **Type:** Release optimization plan.
> **Status:** Proposed. Actionable after the current Docker hotfix has been
> verified on one beta or release tag.

## Problem

The release workflow already builds Linux release archives for `linux-x64`,
`linux-arm64`, and `linux-arm`. The Docker release job currently repeats part of
that work inside `Dockerfile.release` by compiling `ersatzrs-web` and
`ersatzrs-healthcheck` again for each Docker platform.

That duplicate native compile is the slowest and least stable part of the Docker
release path, especially for `linux/arm64` and `linux/arm/v7` when they run
under QEMU. The runtime Docker image only needs the two executables plus the
already-built Leptos assets and static assets:

- `ersatzrs-web`
- `ersatzrs-healthcheck`
- `public/`
- `site/`
- `assets/`

## Goal

Publish the same multi-arch GHCR image without compiling Rust inside the Docker
build. Docker should become a packaging step that consumes already-built release
artifacts from the release archive matrix.

## Non-goals

- Do not change the published Docker image name, channel tags, exposed port,
  user, volume, entrypoint, healthcheck command, or runtime environment
  variables.
- Do not add macOS Docker artifacts.
- Do not bundle FFmpeg or FFprobe into the Docker image.
- Do not weaken the web bundle consistency gate.

## Source Map

- `.github/workflows/release.yml`
  - `web-assets` builds `release-web-assets/`.
  - `binaries` builds the Linux and Windows release archives.
  - `docker` publishes the GHCR image.
- `Dockerfile.release` currently contains both builder and runtime stages.
- `docker-bake.hcl` defines the `ersatzrs-release` target and platform list.
- `scripts/release/verify-web-bundle.sh` proves the SSR binary and WASM assets
  agree for the release bundle.

## Proposed Shape

1. Keep the existing release archive matrix as the source of compiled Linux
   runtime binaries.
2. Upload a small per-RID binary artifact from the Linux rows before archive
   packaging, for example:
   - `docker-binaries-linux-x64`
   - `docker-binaries-linux-arm64`
   - `docker-binaries-linux-arm`
3. Download those artifacts in the Docker job together with `web-assets`.
4. Replace the Docker Rust builder stage with a runtime-only Dockerfile path
   that selects the correct artifact for the current Docker platform.
5. Keep BuildKit/buildx responsible for assembling the image filesystem and
   manifest, not compiling Rust.

The Docker platform to RID mapping should be explicit:

| Docker platform | Release RID | Rust target |
| --- | --- | --- |
| `linux/amd64` | `linux-x64` | `x86_64-unknown-linux-gnu` |
| `linux/arm64` | `linux-arm64` | `aarch64-unknown-linux-gnu` |
| `linux/arm/v7` | `linux-arm` | `armv7-unknown-linux-gnueabihf` |

## Implementation Plan

### Phase 1 - Artifact Contract

- Extend the Linux rows of the `binaries` job to stage the Docker runtime
  binary pair in a predictable directory.
- Upload only `ersatzrs-web` and `ersatzrs-healthcheck` for Docker packaging.
- Keep archive packaging unchanged; the release tarballs should still include
  `ersatzrs-scanner`, docs, assets, public files, and site files.
- Ensure artifact names are deterministic and include the release RID.

### Phase 2 - Runtime-only Dockerfile

- Add a runtime-only Dockerfile path or refactor `Dockerfile.release` so the
  release image copies prebuilt binaries instead of running `cargo build`.
- Preserve the current Debian runtime base, user, `/data` volume, environment
  variables, healthcheck, and entrypoint.
- Preserve the existing web-assets copy behavior for `public/` and `site/`.
- Preserve the `assets/` copy behavior.

### Phase 3 - Workflow Wiring

- Make the Docker job depend on the Linux archive rows or on a dedicated Linux
  binary-artifact job.
- Download the three Linux binary artifacts and `web-assets`.
- Build the same platform set from `docker-bake.hcl`.
- Verify the final manifest still contains `linux/amd64`, `linux/arm64`, and
  `linux/arm/v7`.

### Phase 4 - Cleanup

- Remove the Docker-specific Rust dependency cache and target cache mounts once
  no Docker release build runs Cargo.
- Remove QEMU setup from the Docker job if BuildKit can assemble all runtime
  images without executing target binaries.
- Keep QEMU if any runtime validation step still executes target binaries during
  the Docker build.

## Acceptance Checks

- `Dockerfile.release` or its replacement has no `cargo build` invocation in
  the release image path.
- The Docker job publishes `ghcr.io/...:<version>` and the channel tag exactly
  as before.
- `docker buildx imagetools inspect "$REGISTRY_IMAGE:$VERSION"` lists:
  - `linux/amd64`
  - `linux/arm64`
  - `linux/arm/v7`
- The image contains `ersatzrs-web`, `ersatzrs-healthcheck`, `public/`, `site/`,
  and `assets/`.
- The image does not contain `ersatzrs-scanner`, `ffmpeg`, or `ffprobe`.
- `scripts/release/verify-web-bundle.sh` still runs before Docker publication.

## Risks and Open Questions

- GitHub artifact download paths need to be stable enough for BuildKit context
  selection without relying on shell glob expansion.
- The `linux-arm` RID must map to Docker `linux/arm/v7`; a wrong variant would
  produce an image that pulls but cannot run on the intended devices.
- If Docker cannot copy platform-selected files cleanly from one build context,
  the sequential platform publishing plan can provide the simpler execution
  model.
- The runtime base image architecture must match the copied binary architecture;
  keep the manifest verification and add a smoke check when practical.
