# Persistent Docker Build Cache

> **Type:** Release optimization plan.
> **Status:** Proposed. Useful as a standalone improvement while Docker still
> compiles Rust, and still useful for image assembly layers after Docker becomes
> packaging-only.

## Problem

`Dockerfile.release` already uses BuildKit cache mounts for Cargo registry,
Cargo git, and target output. Those mounts help only when the same builder keeps
its local BuildKit state between attempts.

Repeated beta attempts can still become expensive when the local builder cache
is pruned, the job runs on a different builder, or a failed multi-platform
release leaves useful work trapped in an ephemeral cache. The result is that
the release Docker job may re-download dependencies and recompile more than it
should.

## Goal

Export and import a persistent BuildKit cache for the release Docker target so
repeat beta and release attempts reuse stable layers across workflow runs.

The cache should reduce wasted rebuilds without changing the runtime image
contents or release tags.

## Non-goals

- Do not treat cache hits as correctness evidence; keep static validation and
  manifest checks.
- Do not require cache availability for a release to succeed.
- Do not store secrets in cacheable build layers.
- Do not replace the stronger plan to avoid Rust compilation in Docker.

## Source Map

- `Dockerfile.release`
  - manifest-first dependency layer
  - Cargo registry/git/target cache mounts
  - release runtime copy behavior
- `docker-bake.hcl`
  - `target "ersatzrs-release"`
  - platform list and release tag wiring
- `.github/workflows/release.yml`
  - `docker` job with `docker/bake-action`
  - GHCR login and final manifest verification

## Proposed Shape

Add an explicit persistent cache to the release Docker build. Prefer a registry
cache stored in GHCR because it works across self-hosted runner cache eviction
and across BuildKit builder recreation.

Example cache reference:

```text
ghcr.io/<owner>/<repo>:buildcache
```

The release Docker build should import from that cache and export back to it on
successful progress. If the registry cache is missing or invalid, the build must
fall back to a normal cold build.

## Implementation Plan

### Phase 1 - Choose Cache Backend

- Prefer `type=registry` using GHCR, for example
  `$REGISTRY_IMAGE:buildcache`.
- Evaluate whether `type=gha` is available and useful on the self-hosted release
  runner. If it is less predictable than GHCR for this runner, keep GHCR as the
  primary cache.
- Document the chosen cache tag as an implementation tag, not a user-facing
  runtime image.

### Phase 2 - Wire Cache Import and Export

- Add cache import/export options to the Docker release build path.
- For `docker/bake-action`, add equivalent `cache-from` and `cache-to`
  configuration through the action inputs, bake file, or a release-only bake
  override file.
- Use a cache mode that preserves dependency and target layers well enough for
  repeated beta attempts.
- Keep cache export non-fatal when practical; a failed cache push should not
  invalidate a successfully built release image unless the action cannot express
  that distinction.

### Phase 3 - Keep Runtime Image Clean

- Verify the cache reference is not tagged as `latest`, `beta`, `alpha`, or a
  versioned runtime image.
- Verify the runtime image contents are unchanged.
- Keep the existing checks that the release image does not bundle FFmpeg or
  FFprobe.

### Phase 4 - Measure and Prune

- Record wall-clock time for a cold run and a repeated run.
- Track GHCR package storage growth from the cache tag.
- Define a manual cleanup procedure for the build cache tag if it becomes stale
  or too large.

## Acceptance Checks

- `docker buildx bake --print ersatzrs-release` shows the release cache
  import/export configuration.
- A release Docker build succeeds when the cache tag does not exist.
- A repeated release Docker build reuses cache according to BuildKit logs.
- The final manifest still contains:
  - `linux/amd64`
  - `linux/arm64`
  - `linux/arm/v7`
- Runtime image tags remain limited to version and channel tags.
- The cache tag is documented as internal release infrastructure.

## Risks and Open Questions

- A shared multi-platform cache can grow quickly; GHCR storage should be watched
  after the first few beta attempts.
- Cache poisoning risk is low for this repo-controlled release workflow, but the
  cache should not be imported from untrusted branches.
- Registry cache helps repeated Docker builds, but it does not remove QEMU
  compile fragility by itself. The prebuilt-binary image plan remains the
  stronger reliability fix.
- If Docker becomes packaging-only, this cache still helps base image and asset
  copy layers, but the payoff will be much smaller.
