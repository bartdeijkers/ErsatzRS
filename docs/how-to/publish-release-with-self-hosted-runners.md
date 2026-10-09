# Publish a release without GitHub-hosted minutes

Use this guide for a normal Linux/Windows/Docker release when GitHub-hosted
minutes are unavailable. Do not use a beta tag: the current beta workflow is
Windows-only. Follow [Build and publish a local Windows
release](release-locally.md) for that path.

macOS archives are intentionally omitted so this release path stays focused on
Linux, Windows, and GHCR artifacts.

This produces:

- `ersatzrs-<tag>-linux-x64.tar.gz`
- `ersatzrs-<tag>-linux-arm64.tar.gz`
- `ersatzrs-<tag>-linux-arm.tar.gz`
- `ersatzrs-<tag>-win-x64.zip`
- `ghcr.io/bartdeijkers/ersatzrs:<tag>`
- GitHub-generated source archives

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
docker buildx version
```

Docker or Podman must be available for `cross` to build the ARM Linux archives.
Docker Buildx and QEMU must be available for the GHCR multi-arch image publish
job.

On the Windows runner, install:

- Rust MSVC toolchain.
- Visual Studio Build Tools with the C++ build tools.
- `wasm32-unknown-unknown` target.
- `cargo-leptos 0.3.6`.
- `stylance-cli 0.8.2`.
- 7-Zip available as `7z`.
- Python 3.12 or newer available as `python` in the runner service account's PATH.
  Windows packaging reuses that installation; it does not install Python or
  change PowerShell execution policy.

```powershell
rustup target add wasm32-unknown-unknown
cargo install cargo-leptos --version 0.3.6 --locked
cargo install stylance-cli --version 0.8.2 --locked
cargo leptos --version
stylance --version
7z
```

## Confirm workflow routing

The committed [release workflow](../../.github/workflows/release.yml) is the
canonical job and matrix definition. Before tagging, confirm that:

- release creation, web assets, and Docker publishing target the Linux release
  runner;
- Linux binary entries target the Linux release runner;
- the Windows entry targets the Windows release runner; and
- no active matrix entry expects a macOS runner.

Commit and validate any required workflow correction before tagging. Do not
make an uncommitted runner change on the release host.

Push the final candidate commit and satisfy the mandatory
[release-candidate CI gate](../../BUILD.md#release-candidate-ci-gate) before
creating the tag.

## Push the release tag

Create and push the annotated tag:

```bash
tag=vX.Y.Z
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

Confirm the GHCR image exists and carries the expected Linux platforms:

```bash
docker buildx imagetools inspect "ghcr.io/bartdeijkers/ersatzrs:$tag"
```
