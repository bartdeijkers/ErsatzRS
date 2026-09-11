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
They run only on `[self-hosted, ersatzrs-windows-release]`, stage the
`ersatzrs-<tag>-win-x64.zip` in an unpublished draft prerelease, and do not create
Linux archives or a Docker/GHCR image. Follow the beta acceptance and publication
steps below; a successful beta build does not publish the package.

Do not use `./run.ps1 build` for release packaging. That command is a local dev
one-shot: it builds Leptos assets and starts `ersatzrs-web`. It does not create a
`build/` directory or per-RID release archives.

## Prepare the release commit

Confirm the release commit is ready:

```bash
git status --short
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
./run.sh test-closeout
```

Update `CHANGELOG.md` and any release notes in the same commit if the release
requires them.

Push that commit and satisfy the mandatory
[release-candidate CI gate](../../BUILD.md#release-candidate-ci-gate). Do not
create the tag until the exact candidate SHA has a completed, successful CI
run.

## Push the release tag

Create and push an annotated tag:

```bash
tag=vX.Y.Z
git tag -a "$tag" -m "$tag"
git push origin "$tag"
```

For non-beta tags, the tag push is the release command. It starts `.github/workflows/release.yml`,
which creates the GitHub release, builds the web assets once, builds each active
platform archive, uploads the archives, and publishes the multi-arch Docker
image to GHCR.

For a beta, push a new immutable annotated beta tag only after exact-SHA CI is
green. The tag is visible before manual package acceptance; the release remains
a draft. Failed candidates consume their beta version: never move or reuse a tag.

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

## Accept and publish a Windows beta draft

Wait for the exact tag's Windows release job to finish successfully. Confirm
its commit matches the green CI candidate, then download the draft asset into
a fresh temporary directory:

```bash
gh release view "$tag" --json tagName,isDraft,isPrerelease,assets
candidate_dir=$(mktemp -d)
gh release download "$tag" --pattern "ersatzrs-$tag-win-x64.zip" --dir "$candidate_dir"
sha256sum "$candidate_dir/ersatzrs-$tag-win-x64.zip"
```

Require `isDraft` and `isPrerelease` to be true. Record the ZIP digest, verify
its [bundle contents](../reference/release-bundle.md), extract it, and complete
the [Windows acceptance checklist](../../plans/windows-beta-release-hardening.md#mandatory-acceptance-for-the-next-beta)
and current milestone acceptance against those exact bytes. Native Windows
process tests must also cover that source revision. CI alone is not this proof.

Do not publish while the release workflow or a rerun is active. Rerunning it may
replace a draft asset and invalidates acceptance of the previous ZIP. Immediately
before publication, download the asset again into another fresh directory and
require its SHA-256 to match the tested ZIP. Confirm the tag still resolves to
the intended commit and the release is still a draft. Only after every required
gate passes, publish the existing draft:

```bash
gh release edit "$tag" --draft=false
```

Do not rebuild or upload an archive during publication. Re-download the public
asset and require the same digest; verify the release is a prerelease and no
longer a draft. The beta workflow refuses to overwrite published release assets.

## Verify the published non-beta release

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
