# Publish a full release

Use this guide to publish a full release through the release workflow. This
produces the GitHub release archives:

- `ersatzrs-<tag>-linux-x64.tar.gz`
- `ersatzrs-<tag>-linux-arm64.tar.gz`
- `ersatzrs-<tag>-linux-arm.tar.gz`
- `ersatzrs-<tag>-osx-arm64.tar.gz`
- `ersatzrs-<tag>-osx-x64.tar.gz`
- `ersatzrs-<tag>-win-x64.zip`
- GitHub-generated source archives

The same workflow also publishes the multi-arch Docker image to GHCR.

Do not use `./run.ps1 build` for release packaging. That command is a local dev
one-shot: it builds Leptos assets and starts `ersatzrs-web`. It does not create a
`build/` directory, per-RID release archives, or a Docker image.

## Prepare the release commit

Confirm the release commit is ready:

```bash
git status --short
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
cargo test --workspace --all-features
```

Update `CHANGELOG.md` and any release notes in the same commit if the release
requires them.

## Push the release tag

Create and push an annotated tag:

```bash
tag=v0.0.6-alpha
git tag -a "$tag" -m "$tag"
git push origin "$tag"
```

The tag push is the release command. It starts `.github/workflows/release.yml`,
which creates the GitHub release, builds the web assets once, builds each
platform archive, uploads the archives, and publishes the Docker image.

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

Confirm the release has all six binary archives:

```bash
gh release view "$tag" --json tagName,assets,url
```

Confirm the Docker image manifest includes the expected Linux platforms:

```bash
docker buildx imagetools inspect "ghcr.io/bartdeijkers/ersatzrs:$tag"
```

The manifest should include:

- `linux/amd64`
- `linux/arm64`
- `linux/arm/v7`

## When GitHub-hosted minutes are exhausted

If hosted Actions minutes are exhausted, use
[Publish a release without GitHub-hosted minutes](publish-release-with-self-hosted-runners.md).
That path keeps the release workflow but runs it on self-hosted Linux and
Windows runners, with macOS archives temporarily omitted.

## When GitHub Actions cannot build everything

Use [Build and publish a local Windows release](release-locally.md) only as a
fallback for the Windows archive. A local Windows fallback does not produce the
Linux archives, macOS archives, or Docker image.
