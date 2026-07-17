# Build and publish a local Windows release

Use this guide when GitHub Actions cannot produce the release artifacts and you
need to build the `win-x64` archive locally, push the tag, and attach the archive
to the GitHub release.

This guide assumes you are intentionally producing only the Windows archive.
Use a beta tag (`v*.*.*-beta.*`) when the active Windows runner should publish
the archive: that tag runs the Windows-only prerelease workflow and does not
produce Linux archives or a Docker image.

For a normal release that produces the Linux and Windows GitHub assets, use
[Publish a full release](publish-release.md). Do not use `./run.ps1 build` for
release packaging; it builds local dev assets and starts the server, but it does
not create release archives or a `build/` directory.

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
$Tag = "v1.0.15-beta.8"
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
Copy-Item -Recurse crates\ersatzrs-web\public $Stage\public
Copy-Item -Recurse assets $Stage\assets
Copy-Item LICENSE $Stage\
Copy-Item README.md $Stage\
```

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

Create and push an annotated tag:

```powershell
git tag -a $Tag -m $Tag
git push origin $Tag
```

The tag push starts the release workflow. If the workflow creates the release,
upload the local archive to that release:

```powershell
gh release upload $Tag "$Stage.zip" --clobber
```

If the workflow did not create a release, create it from the pushed tag and
attach the archive:

```powershell
gh release create $Tag "$Stage.zip" --generate-notes --verify-tag
```

Verify the published release includes the local archive:

```powershell
gh release view $Tag --json tagName,assets,url
```
