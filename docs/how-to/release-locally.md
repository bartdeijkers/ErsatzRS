# Build and publish a local Windows release

Use this guide when GitHub Actions cannot produce the release artifacts and you
need to build the `win-x64` archive locally, push the tag, and attach the archive
to the GitHub release.

This guide assumes you are intentionally producing only the Windows archive.
Use a beta tag (`v*.*.*-beta.*`) when the active Windows runner should build
the archive: that tag runs the Windows-only draft-prerelease workflow and does not
produce Linux archives or a Docker image.

This fallback does not replace the normal multi-platform release workflow. Do
not use `./run.ps1 build` for release packaging; it builds local development
assets and starts the server, but it does not create release archives or a
`build/` directory.

## Prerequisites

- Rust with the `wasm32-unknown-unknown` target installed.
- `cargo-leptos 0.3.6`.
- `stylance-cli 0.8.2`.
- GitHub CLI (`gh`) authenticated for this repository.
- 7-Zip available as `7z`.
- The repository `.cargo/config.toml` target directory in use. Do not override
  `CARGO_TARGET_DIR`; use the configured target directory for this checkout.

Check the tool versions before building:

```powershell
cargo leptos --version
stylance --version
gh auth status
rustup target list --installed | Select-String wasm32-unknown-unknown
$TargetRoot = "<configured-target-dir>"
```

## Build the frontend assets

Build the Leptos hydration bundle from the web crate:

```powershell
Set-Location crates/ersatzrs-web
cargo leptos build --release --frontend-only --lib-features hydrate
Set-Location ../..
```

Confirm the expected outputs exist under the configured target directory and the
web crate public assets:

```powershell
Test-Path "$TargetRoot\site\pkg"
Test-Path crates\ersatzrs-web\public\bundle.css
```

## Build the release binaries

Build the Windows server, scanner, healthcheck, and launcher binaries:

```powershell
cargo build --release `
  --bin ersatzrs-web `
  --bin ersatzrs-scanner `
  --bin ersatzrs-healthcheck `
  --bin ersatzrs-launcher `
  --target x86_64-pc-windows-msvc
```

## Stage the archive

Set the release tag and create the workflow-compatible stage directory:

```powershell
$Tag = "vX.Y.Z-beta.N"
$Stage = "ersatzrs-$Tag-win-x64"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory $Stage | Out-Null
```

Copy the binaries, naming the launcher as the user-facing entry point:

```powershell
Copy-Item "$TargetRoot\x86_64-pc-windows-msvc\release\ersatzrs-web.exe" $Stage\
Copy-Item "$TargetRoot\x86_64-pc-windows-msvc\release\ersatzrs-scanner.exe" $Stage\
Copy-Item "$TargetRoot\x86_64-pc-windows-msvc\release\ersatzrs-healthcheck.exe" $Stage\
Copy-Item "$TargetRoot\x86_64-pc-windows-msvc\release\ersatzrs-launcher.exe" $Stage\ErsatzRS.exe
```

Copy the web assets and repository assets:

```powershell
Copy-Item -Recurse "$TargetRoot\site" $Stage\site
Copy-Item -Recurse -Force crates\ersatzrs-web\public\* $Stage\site
Copy-Item -Recurse assets $Stage\assets
Copy-Item LICENSE $Stage\
Copy-Item README.md $Stage\
```

Stage the exact pinned Next runtime with the same checksum, payload and provenance
validation as release CI. Move its executable and Windows companions beside the
server; keep the license and origin record under `next/`:

```powershell
python scripts/release/stage-next.py --rid win-x64 --output "$Stage/next"
if ($LASTEXITCODE -ne 0) { throw 'Next runtime staging failed' }
Move-Item "$Stage/next/ersatztv-channel.exe" "$Stage/ersatztv-channel.exe" -ErrorAction Stop
Move-Item "$Stage/next/libvpl.dll" "$Stage/libvpl.dll" -ErrorAction Stop
Move-Item "$Stage/next/libvpl-LICENSE.txt" "$Stage/libvpl-LICENSE.txt" -ErrorAction Stop
foreach ($Required in @('ersatztv-channel.exe', 'libvpl.dll', 'libvpl-LICENSE.txt', 'next/LICENSE', 'next/origin.json')) {
    $File = Get-Item "$Stage/$Required" -ErrorAction Stop
    if ($File.Length -eq 0) { throw "Required Next runtime file is empty: $Required" }
}
```

The staging helper takes the release identity and hashes from the application's
pinned runtime manifest. Its package-only distribution destination is the public
`bartdeijkers/ErsatzRS-next` repository; ordinary runtime downloads require no
GitHub credentials. Preserve exact package/source/patch checksums when publishing
a new accepted runtime there. Do not substitute a mutable tag or locally compiled
Next binary. A standalone source-built installation acquires the same pinned
runtime during startup when no explicit override or bundled runtime is present.

Create the zip:

```powershell
7z a -tzip "$Stage.zip" $Stage
```

The staged layout should match [the release bundle reference](../reference/release-bundle.md).

## Publish to GitHub

Commit and push any release fixes before tagging. Stage only files that belong
to the release fix:

```powershell
git status --short
git add path\to\changed-file
git commit -m "fix(release): prepare local Windows archive"
git push origin HEAD
```

Before creating the tag, satisfy the mandatory
[release-candidate CI gate](../../BUILD.md#release-candidate-ci-gate). A local
archive or focused browser run does not replace successful CI for the exact
candidate SHA.

Create and push an annotated tag:

```powershell
git tag -a $Tag -m $Tag
git push origin $Tag
```

The tag push starts the release workflow. Wait for it to finish before any
fallback upload; never race the runner. For a beta, prefer the runner's draft
archive. Only use the local archive if the workflow could not produce one.
Verify it follows the same bundle-consistency and exact-source acceptance
requirements. Stage it through the same guard used by the beta workflow:

```powershell
./scripts/release/stage-beta-draft.ps1 -Repository '<owner>/<repo>' -Tag $Tag -Archive "$Stage.zip"
```

The guard creates only a draft prerelease and refuses to overwrite a published
release. Replacing a draft ZIP invalidates previous package acceptance. Follow
[Accept and publish a Windows beta draft](publish-release.md#accept-and-publish-a-windows-beta-draft)
to download, test, and publish those exact bytes without another upload.
