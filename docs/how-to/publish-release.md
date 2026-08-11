# Publish a full release

Use this guide to publish a non-beta release through the release workflow. The
active matrix currently produces the GitHub release archives and Docker image:

- `ersatzrs-<tag>-linux-x64.tar.gz`
- `ersatzrs-<tag>-linux-arm64.tar.gz`
- `ersatzrs-<tag>-linux-arm.tar.gz`
- `ersatzrs-<tag>-win-x64.zip`
- `ghcr.io/bartdeijkers/ersatzrs:<tag>`
- `ghcr.io/bartdeijkers/ersatzrs:latest` for normal tags, or
  `ghcr.io/bartdeijkers/ersatzrs:alpha` for alpha tags
- GitHub-generated source archives

macOS archives are intentionally omitted for now. Keep this release line focused
on Linux and Windows archives plus GHCR Docker images; restore the two `osx-*`
matrix entries only in a future macOS-targeted release.

Beta tags (`v*.*.*-beta` or `v*.*.*-beta.*`) intentionally take a separate
Windows-only path.
They run only on `[self-hosted, ersatzrs-windows-release]`, publish the
`ersatzrs-<tag>-win-x64.zip` prerelease, and do not create Linux archives or a
Docker/GHCR image. This keeps beta validation tied to the active Windows runner.

Do not use `./run.ps1 build` for release packaging. That command is a local dev
one-shot: it builds Leptos assets and starts `ersatzrs-web`. It does not create a
`build/` directory or per-RID release archives.

## Prepare the release commit

Confirm the release commit is ready:

```bash
git status --short
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
./run.sh test-closeout
```

Update `CHANGELOG.md` and any release notes in the same commit if the release
requires them.

## Push the release tag

Create and push an annotated tag:

```bash
tag=vX.Y.Z
git tag -a "$tag" -m "$tag"
git push origin "$tag"
```

The tag push is the release command. It starts `.github/workflows/release.yml`,
which creates the GitHub release, builds the web assets once, builds each active
platform archive, uploads the archives, and publishes the multi-arch Docker
image to GHCR.

## Watch the workflow

Wait for the release workflow to finish:

```bash
gh run list --workflow release.yml --limit 1
gh run watch
```

If `gh run watch` selects the wrong run, open the run from the first command and
watch that run explicitly:

```bash
gh run watch <run-id>
```

## Verify the published release

Confirm the release has the active binary archives:

```bash
gh release view "$tag" --json tagName,assets,url
```

Confirm the GHCR image exists and carries the expected Linux platforms:

```bash
docker buildx imagetools inspect "ghcr.io/bartdeijkers/ersatzrs:$tag"
channel=latest # use alpha for an alpha tag
docker buildx imagetools inspect "ghcr.io/bartdeijkers/ersatzrs:$channel"
```

## When GitHub-hosted minutes are exhausted

If you need to restore macOS archives for a future release, re-add the two
`osx-*` matrix entries and use GitHub-hosted macOS runners or a macOS
self-hosted runner. The current Linux/Windows/Docker matrix avoids
GitHub-hosted minutes.

## When GitHub Actions cannot build everything

A local Windows fallback can restore only the Windows archive; it does not
produce the Linux archives.
