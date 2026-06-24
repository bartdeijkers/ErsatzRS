# Publish a release without GitHub-hosted minutes

Use this guide for the current Linux/Windows release path. macOS archives and
Docker/GHCR images are intentionally omitted so the active release line stays
focused on Linux and Windows artifacts.

This produces:

- `ersatzrs-<tag>-linux-x64.tar.gz`
- `ersatzrs-<tag>-linux-arm64.tar.gz`
- `ersatzrs-<tag>-linux-arm.tar.gz`
- `ersatzrs-<tag>-win-x64.zip`
- GitHub-generated source archives

It intentionally does not produce:

- `ersatzrs-<tag>-osx-arm64.tar.gz`
- `ersatzrs-<tag>-osx-x64.tar.gz`
- Docker/GHCR images

## Prepare the runners

Use one Linux runner and one Windows runner.

Recommended labels:

| Runner | Required host | Labels |
|---|---|---|
| Linux release runner | WSL Ubuntu 24.04 or Linux host with Docker | `self-hosted`, `ersatzrs-linux-release` |
| Windows release runner | Windows x64 host with Rust MSVC tooling | `self-hosted`, `ersatzrs-windows-release` |

The Linux runner handles release creation, web assets, and Linux archives. The
Windows runner handles the Windows archive.

Add the Windows runner from GitHub:

1. Open the repository on GitHub.
2. Go to Settings -> Actions -> Runners.
3. Select New self-hosted runner.
4. Select Windows and x64.
5. Run the commands GitHub shows on the Windows machine.
6. Add the custom label `ersatzrs-windows-release`.

When installing the Windows runner as a service, run the setup shell as
administrator and use a stable directory such as `C:\actions-runner`.

## Install runner prerequisites

On the Linux runner:

```bash
rustup target add wasm32-unknown-unknown
cargo install cargo-leptos --version 0.3.6 --locked
cargo install stylance-cli --version 0.8.2 --locked
cargo install cross --locked
docker --version
```

Docker or Podman must be available for `cross` to build the ARM Linux archives;
the release workflow does not publish a Docker image.

On the Windows runner, install:

- Rust MSVC toolchain.
- Visual Studio Build Tools with the C++ build tools.
- `wasm32-unknown-unknown` target.
- `cargo-leptos 0.3.6`.
- `stylance-cli 0.8.2`.
- 7-Zip available as `7z`.

```powershell
rustup target add wasm32-unknown-unknown
cargo install cargo-leptos --version 0.3.6 --locked
cargo install stylance-cli --version 0.8.2 --locked
cargo leptos --version
stylance --version
7z
```

## Release workflow shape

The committed `.github/workflows/release.yml` already uses the self-hosted
Linux/Windows release runner shape and omits macOS archives and Docker
publication. Keep that shape for this release line.

In `.github/workflows/release.yml`:

1. The `release` job should use:

```yaml
runs-on: [self-hosted, ersatzrs-linux-release]
```

2. The `web-assets` job should use:

```yaml
runs-on: [self-hosted, ersatzrs-linux-release]
```

3. In the `binaries` matrix:

- Set the Linux entries to use the Linux release runner.
- Set the Windows entry to use the Windows release runner.
- Do not add the two macOS entries unless this is a future macOS-targeted
  release.

Use this matrix shape:

```yaml
matrix:
  include:
    - rid: linux-x64
      runner: '["self-hosted","ersatzrs-linux-release"]'
      target: x86_64-unknown-linux-gnu
      cross: false
      archive: tar.gz
    - rid: linux-arm64
      runner: '["self-hosted","ersatzrs-linux-release"]'
      target: aarch64-unknown-linux-gnu
      cross: true
      archive: tar.gz
    - rid: linux-arm
      runner: '["self-hosted","ersatzrs-linux-release"]'
      target: armv7-unknown-linux-gnueabihf
      cross: true
      archive: tar.gz
    - rid: win-x64
      runner: '["self-hosted","ersatzrs-windows-release"]'
      target: x86_64-pc-windows-msvc
      cross: false
      archive: zip
```

Then change the `binaries` job `runs-on` to:

```yaml
runs-on: ${{ fromJSON(matrix.runner) }}
```

Commit any workflow change before tagging the release.

## Push the release tag

Create and push the annotated tag:

```bash
tag=v0.0.6-alpha
git tag -a "$tag" -m "$tag"
git push origin "$tag"
```

Watch the workflow:

```bash
gh run list --workflow release.yml --limit 1
gh run watch <run-id>
```

## Verify the result

Confirm the release has the expected non-macOS archives:

```bash
gh release view "$tag" --json tagName,assets,url
```

There is no Docker/GHCR image for this release line.

## Restoring macOS archives later

For a future macOS-targeted release, restore the two `osx-*` matrix entries and
use GitHub-hosted macOS runners or add a macOS self-hosted runner. Do this in a
separate release-prep commit before tagging that release.
