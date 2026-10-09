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
which creates an unpublished GitHub draft, builds the web assets once, builds
each active platform archive, uploads the archives, and publishes the multi-arch
Docker image to GHCR. Publishing the application draft is a separate final step
after archive acceptance; a green build alone does not publish it.

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

## Accept and publish a non-beta draft

After all release jobs succeed, confirm the tag resolves to the exact source
that passed candidate CI. Download all four draft archives into a fresh directory,
record their SHA-256 hashes, and verify the [bundle contents](../reference/release-bundle.md).
Run the web-bundle consistency gate and the current milestone's package/runtime
acceptance against those extracted files. Preserve explicitly approved platform
acceptance debt without describing it as verified.

Confirm no release job or rerun is active, then download the draft archives again
and require their hashes to match the tested files. Publish those existing bytes:

```bash
gh release edit "$tag" --draft=false --prerelease=false --latest
```

Re-download the published archives and compare the hashes again. Do not rebuild
or replace assets during publication. Verify the release is neither a draft nor
a prerelease. The Docker image and channel tag are published by the workflow;
verify their manifest and supported platforms separately.

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

### Recover a Docker build recipe failure

If the archive source has passed CI but the Docker build recipe fails, preserve
the release tag and successful archives. Correct and verify the Dockerfile on
main, push the correction, and wait for its CI. Once the original release run
has completed, dispatch `release-docker-recovery.yml` with the existing stable
`tag`, its full `source_sha`, and the original release `assets_run` ID.

The recovery workflow checks out the immutable application tag and replaces only
`Dockerfile.release` with the corrected recipe. It requires an unpublished stable
draft, successful CI for the application source, and a successful web-assets job
from the matching release run. It reuses that web bundle, records the recipe diff,
and labels the image with both source and recipe revisions. It publishes and
verifies the three-platform image and matching `latest` manifest. Include the
recovery run and recipe revision in release acceptance evidence; the failed
original Docker job is not a successful build.

### Recover a Windows Python setup failure

If the original Windows job passed its native build and startup tests but failed
only at Python setup, install Python 3.12 or newer in the Windows runner account's
PATH. Preserve the accepted application tag, successful Linux archives and shared
web-assets artifact. Commit and push the workflow correction, wait for its green
CI, then dispatch `release-windows-recovery.yml` with the existing stable `tag`,
its full `source_sha`, and the original release `assets_run` ID.

The recovery requires an unpublished stable draft, complete successful CI for the
exact application source, successful matching web assets, and the original Windows
job's terminal Python setup failure. Other original platform jobs may continue;
do not rerun the original Windows job in parallel. Recovery checks out only the
accepted application source with the warm target preserved, repeats the native
build and startup tests, verifies the matched web bundle and pinned Next staging,
and replaces only the Windows draft asset. Its offline Next tests come from the
corrected recovery revision and run against the accepted application's unchanged
staging implementation and manifest; they check bytes/provenance on every host
and POSIX execute permissions only on POSIX hosts. It rechecks the guards before upload
and records the archive checksum, application source and recovery workflow
revision. Include the green recovery run in release acceptance evidence; the
original failed Windows job remains a failure. Perform actual packaged Windows
startup and playback acceptance before publishing the stable draft.
