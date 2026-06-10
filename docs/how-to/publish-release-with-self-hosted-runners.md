# Publish a release without GitHub-hosted minutes

Use this guide when GitHub-hosted Actions minutes are exhausted and the release
can temporarily omit macOS archives. The release still runs through GitHub
Actions, but the build jobs run on self-hosted machines.

This produces:

- `ersatzrs-<tag>-linux-x64.tar.gz`
- `ersatzrs-<tag>-linux-arm64.tar.gz`
- `ersatzrs-<tag>-linux-arm.tar.gz`
- `ersatzrs-<tag>-win-x64.zip`
- GitHub-generated source archives
- The multi-arch Docker image on GHCR

It intentionally does not produce:

- `ersatzrs-<tag>-osx-arm64.tar.gz`
- `ersatzrs-<tag>-osx-x64.tar.gz`

## Prepare the runners

Use one Linux runner and one Windows runner.

Recommended labels:

| Runner | Required host | Labels |
|---|---|---|
| Linux release runner | WSL Ubuntu 24.04 or Linux host with Docker | `self-hosted`, `ersatzrs-linux-release` |
| Windows release runner | Windows x64 host with Rust MSVC tooling | `self-hosted`, `ersatzrs-windows-release` |

The Linux runner handles release creation, web assets, Linux archives, and the
Docker image. The Windows runner handles the Windows archive.

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
docker buildx version
```

Docker must support `buildx` and QEMU for the Docker image and ARM Linux
archives.

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

## Patch the release workflow for self-hosted runners

Use a short-lived release branch. Do not keep the macOS entries removed after the
hosted-minute emergency is over.

In `.github/workflows/release.yml`:

1. Change the `release` job to:

```yaml
runs-on: [self-hosted, ersatzrs-linux-release]
```

2. Change the `web-assets` job to:

```yaml
runs-on: [self-hosted, ersatzrs-linux-release]
```

3. In the `binaries` matrix:

- Set the Linux entries to use the Linux release runner.
- Set the Windows entry to use the Windows release runner.
- Remove the two macOS entries.

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

4. Change the `docker` job to:

```yaml
runs-on: [self-hosted, ersatzrs-linux-release]
```

Commit this workflow patch before tagging the release.

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

Confirm the Docker image manifest:

```bash
docker buildx imagetools inspect "ghcr.io/bartdeijkers/ersatzrs:$tag"
```

The manifest should include:

- `linux/amd64`
- `linux/arm64`
- `linux/arm/v7`

## Restore the normal workflow

After the release, revert the temporary workflow patch or restore the macOS
matrix entries before the next normal release.
