# Build FFmpeg 8.1 in the release workflow

## Context

The Windows release workflow currently downloads a public FFmpeg archive and
stages `ffmpeg.exe` / `ffprobe.exe` next to the launcher. That is too weak for
ErsatzRS: startup requires both tools to report a parseable FFmpeg version
`>= 8.1`, the `drawvg` filter (via `--enable-cairo`), and the `zscale` filter
(via `--enable-libzimg`) for the HDR/tonemap color pipeline. Recent
BtbN/Gyan-style public builds either missed those capabilities or did not
produce version/configuration output that the runtime checker accepted.

Replace the public download with a first-party FFmpeg 8.1 build in GitHub
Actions. The release archive should carry the exact runtime that the app
validates.

## Goals

- Build FFmpeg 8.1 from official source during the release workflow.
- Produce a relocatable payload at `ffmpeg/bin/{ffmpeg,ffprobe}[.exe]`.
- Include the runtime library closure needed by the payload on each platform.
- Run the same runtime checks that ErsatzRS uses before uploading artifacts:
  parseable `ffmpeg -version` / `ffprobe -version`, `--enable-cairo`, `drawvg`,
  `--enable-libzimg`, `zscale`, required codecs, a real `drawvg` smoke
  transcode, and a real `zscale`/`tonemap` smoke transcode.
- Remove all BtbN/Gyan download assumptions from `.github/workflows/release.yml`
  and release docs.
- Keep environment overrides higher priority than bundled binaries.

## Non-Goals

- Do not change the graphics-engine runtime requirements.
- Do not lower the required FFmpeg version below 8.1.
- Do not depend on a user's system FFmpeg for release archives.
- Do not solve the existing DB-configured FFmpeg path gap in this slice.

## Workflow Shape

Add a new `build-ffmpeg` job before `binaries`:

```text
release
  |
  +-- build-ffmpeg matrix
  |     -> uploads ffmpeg-linux-x64
  |     -> uploads ffmpeg-linux-arm64
  |     -> uploads ffmpeg-linux-arm
  |     -> uploads ffmpeg-osx-x64
  |     -> uploads ffmpeg-osx-arm64
  |     -> uploads ffmpeg-win-x64
  |
  +-- binaries matrix
        -> downloads matching ffmpeg-<rid>
        -> stages archive with ffmpeg/bin payload
```

Change `binaries.needs` from `release` to `[release, build-ffmpeg]`.

## Platform Builds

### Linux

Reuse the existing Docker `ffmpeg-build` recipe as the source of truth for
Linux configure flags.

- Add `--extra-ldflags='-Wl,-rpath,$ORIGIN/../lib'` to the Linux FFmpeg
  configure invocation.
- Add an `ffmpeg-export` Docker target that copies `/opt/ffmpeg` into
  `/ffmpeg` and vendors the non-baseline shared-library closure into
  `/ffmpeg/lib`.
- Build each Linux RID through `docker buildx build --target ffmpeg-export`
  with `--output type=local,dest=out`.
- Upload `out/ffmpeg` as artifact `ffmpeg-<rid>`.
- Verify from a clean container that `ffmpeg/bin/ffmpeg` has no unresolved
  `ldd` entries and resolves non-baseline dependencies from `ffmpeg/lib`.

### Windows

Build FFmpeg 8.1 from source on `windows-latest` using MSYS2 UCRT64 or MINGW64.

- Install cairo, fontconfig, freetype, fribidi, harfbuzz, libass, x264, x265,
  svt-av1, dav1d, aom, vpx, opus, lame, vorbis, webp, zimg, OpenSSL, nasm,
  yasm, pkg-config, and the compiler toolchain.
- Configure with the common required flags:
  `--enable-cairo`, `--enable-libzimg`, `--enable-libfontconfig`,
  `--enable-libass`, `--enable-libfreetype`, `--enable-libfribidi`,
  `--enable-libharfbuzz`, `--enable-gpl`, `--enable-version3`,
  `--enable-shared`, `--enable-ffprobe`.
- Include codec/library flags that map to the installed MSYS2 packages.
- Exclude Linux-only flags such as `--enable-libdrm`, `--enable-libv4l2`,
  `--enable-vaapi`, and Linux rpath linker syntax.
- Copy `ffmpeg.exe`, `ffprobe.exe`, and all required DLLs into `ffmpeg/bin`.
- Verify the relocated payload from a shell where MSYS2 bin directories are not
  implicitly satisfying missing DLLs.

### macOS

Build FFmpeg 8.1 from source on native macOS runners.

- Use an Intel runner for `osx-x64` and an arm64 runner for `osx-arm64`.
- Install required dependencies via Homebrew, including `zimg` (Homebrew
  formula `zimg`) so `--enable-libzimg`/`zscale` is available.
- Configure with the common cairo/zimg/libass/font stack and codecs available
  on Homebrew, including `--enable-libzimg`.
- Exclude Linux-only flags such as `libdrm`, `libv4l2`, `vaapi`, CUDA/QSV, and
  Linux `TARGETARCH`-gated accelerator options.
- Relocate non-system dylibs to `@loader_path/../lib` and verify with
  `otool -L ffmpeg/bin/ffmpeg`.
- Upload `ffmpeg/bin` plus the relocated dylib closure.

## Required Validation

Every `build-ffmpeg` matrix cell must fail before upload unless these checks
pass against the relocated payload:

```bash
ffmpeg/bin/ffmpeg -version
ffmpeg/bin/ffprobe -version
ffmpeg/bin/ffmpeg -hide_banner -buildconf | grep -- --enable-cairo
ffmpeg/bin/ffmpeg -hide_banner -filters | grep -w drawvg
ffmpeg/bin/ffmpeg -hide_banner -buildconf | grep -- --enable-libzimg
ffmpeg/bin/ffmpeg -hide_banner -filters | grep -w zscale
ffmpeg/bin/ffmpeg -hide_banner \
  -f lavfi -i color=c=black:s=32x32:d=0.1 \
  -vf "drawvg=script='rect 4 4 16 16 fill',format=yuv420p" \
  -frames:v 1 -f null -
ffmpeg/bin/ffmpeg -hide_banner -loglevel error \
  -f lavfi -i "color=c=black:s=64x64:d=0.1,format=yuv420p10le,setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc" \
  -vf "zscale=t=linear:npl=100,tonemap=hable,zscale=p=bt709:t=bt709:m=bt709,format=yuv420p" \
  -frames:v 1 -f null -
```

The synthetic source must be tagged with HDR color metadata
(`setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc`);
otherwise `zscale` reports "no path between colorspaces" even on a correct zimg
build and the smoke check fails spuriously.

Also keep the existing release capability reports for Docker images. The release
archive validation should be explicit and independent from image validation.

## Runtime Discovery

Update runtime path resolution so release archives can be self-contained:

1. Existing env var override.
2. Bundled binary next to the executable:
   - `<exe_dir>/ffmpeg/bin/ffmpeg[.exe]`
   - `<exe_dir>/ffmpeg/bin/ffprobe[.exe]`
3. Legacy sibling fallback for transition:
   - `<exe_dir>/ffmpeg[.exe]`
   - `<exe_dir>/ffprobe[.exe]`
4. PATH fallback: `ffmpeg` / `ffprobe`.

Apply this in:

- `crates/ersatzrs-ffmpeg/src/version.rs`
- `crates/ersatzrs-scanner/src/main.rs` for scanner `ffprobe` discovery

Add resolver tests that prove env vars win, bundled paths are discovered, and
PATH fallback remains intact.

## Release Staging

In `.github/workflows/release.yml`:

- Delete the `Bundle FFmpeg 8.1 with drawvg (win-x64)` download step.
- Add `actions/download-artifact` for `ffmpeg-${{ matrix.rid }}`.
- Stage the downloaded `ffmpeg/` directory into every archive, including
  Windows.
- Prefer `ffmpeg/bin/` over root-level `ffmpeg.exe` / `ffprobe.exe`.
- Keep the launcher as `ErsatzRS.exe` in the Windows archive.
- Ensure the Windows archive still includes `site/`, `assets/`, `LICENSE`, and
  `README.md`.

Expected archive shape:

```text
ersatzrs-vX.Y.Z-<rid>/
  ersatzrs-web[.exe]
  ersatzrs-scanner[.exe]
  ErsatzRS.exe                # Windows only
  ffmpeg/
    bin/
      ffmpeg[.exe]
      ffprobe[.exe]
      *.dll / *.dylib / platform libs as needed
    lib/                      # Linux/macOS when needed
  site/                       # Windows only for launcher UI
  assets/
  LICENSE
  README.md
```

## Documentation Updates

- `RELEASING.md`: describe the bundled self-built FFmpeg payload, discovery
  order, and local smoke checks.
- `plans/bundling-ffmpeg.md`: either link to this workflow-focused plan or mark
  this as the implementation slice for replacing public downloads.
- `CHANGELOG.md`: add an Unreleased entry once implementation starts.

## Acceptance Criteria

- Release workflow no longer references BtbN or Gyan download URLs.
- All six release RIDs upload an `ffmpeg-<rid>` artifact.
- All release archives contain `ffmpeg/bin/{ffmpeg,ffprobe}[.exe]`.
- Windows launcher starts from the zip on a machine without system FFmpeg.
- `ffmpeg -version` and `ffprobe -version` are parseable by
  `crates/ersatzrs-ffmpeg/src/version.rs`.
- `--enable-cairo`, `drawvg`, `--enable-libzimg`, and `zscale` are checked in
  CI before artifact upload, and a `zscale`/`tonemap` smoke transcode passes, so
  a drawvg-only (zimg-less) FFmpeg build cannot produce a release archive.
- Linux exported payload runs in a clean container without `LD_LIBRARY_PATH`.
- macOS payload runs without `DYLD_LIBRARY_PATH`.
- Windows payload runs without adding MSYS2 directories to `PATH`.

## Suggested Implementation Order

1. Add runtime bundled-path discovery and tests.
2. Add Docker `ffmpeg-export` target and prove linux-x64 locally.
3. Add `build-ffmpeg` job for linux-x64 only.
4. Wire artifact download into `binaries` staging.
5. Extend `build-ffmpeg` to linux-arm64 and linux-arm.
6. Add Windows source build and DLL closure staging.
7. Add macOS x64/arm64 source builds and dylib relocation.
8. Update release docs and remove stale BtbN/Gyan wording.
9. Run a throwaway tag release and inspect all archives.
