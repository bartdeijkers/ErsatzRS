# Bundle an optimized FFmpeg 8.1 in ErsatzRS release archives

## Context

ErsatzRS release archives (`.github/workflows/release.yml`, "Stage archive")
ship only `ersatzrs-web` + `ersatzrs-scanner` + `assets/` + `LICENSE` +
`README.md`. They rely on a correct FFmpeg being on `PATH` or pointed at by an
env var. Unlike ErsatzTV, ErsatzRS has a **hard requirement that off-the-shelf
FFmpeg cannot satisfy**: `crates/ersatzrs-ffmpeg/src/version.rs` fails fast at
startup unless `ffmpeg`/`ffprobe` are **>= 8.1** *and* were configured with
`--enable-cairo` (plus libass/freetype/harfbuzz/fontconfig) and expose the
`drawvg` filter (see `REQUIRED_FILTERS`/`REQUIRED_CONFIGURATION_FLAGS` at
`version.rs:23-32` and `check_ffmpeg_runtime_with_config`). Public builds
(BtbN, GyanD, distro packages) do **not** enable cairo/drawvg, so users
frequently cannot run ErsatzRS at all.

This change bundles a self-contained, cairo-enabled FFmpeg 8.1 inside each
release archive and teaches the runtime resolvers to discover it next to the
executable — adopting ErsatzTV's *bundle-next-to-binary + discover-relative-to-exe*
pattern, but building our own binaries since ErsatzTV's (7.1.1, no cairo) are
unusable. The Docker `ffmpeg-build` stage (`docker/Dockerfile:114-184`) already
builds exactly the FFmpeg we need and stays the single source of truth for
build flags.

## Shape of the change

```
                 release.yml
  ┌────────────────────────────────────────────────────────┐
  │  release ──▶ build-ffmpeg (matrix×6) ──▶ binaries (matrix×6) │
  │                  │ artifact ffmpeg-<rid>      │              │
  │                  ▼                            ▼              │
  │           ffmpeg/bin/{ffmpeg,ffprobe}   download + copy into  │
  │           + co-located libs (rpath)     $stage/ffmpeg/        │
  └────────────────────────────────────────────────────────┘
                                              │ archive
                                              ▼
   ersatzrs-web ─┐                    ┌─ <exedir>/ffmpeg/bin/ffmpeg
   ersatzrs-scanner ─ resolver order:─┤  1. env var override (unchanged)
                                      │  2. NEW: bundled, relative to current_exe()
                                      └─ 3. "ffmpeg"/"ffprobe" on PATH (unchanged)
                                              │
                                              ▼
                          version.rs startup check = the verifier
                          (drawvg/cairo binaries pass it unchanged)
```

## 1. Runtime: bundled-path discovery (`crates/ersatzrs-ffmpeg/src/version.rs`)

Insert a bundled-discovery step between the existing env lookup and the `PATH`
default. Resolution order becomes: **env var → bundled-relative-to-exe → PATH**.

- Add a helper that mirrors the sibling-binary pattern already used in
  `crates/ersatzrs-infra/src/maintenance.rs:576-588`:
  ```rust
  fn discover_bundled(tool: &str) -> Option<String> {
      let exe = std::env::current_exe().ok()?;
      let dir = exe.parent()?;
      let name = if cfg!(target_os = "windows") {
          format!("{tool}.exe")
      } else {
          tool.to_owned()
      };
      // <exedir>/ffmpeg/bin/<tool> then <exedir>/<tool>
      for candidate in [dir.join("ffmpeg").join("bin").join(&name), dir.join(&name)] {
          if candidate.is_file() {
              return candidate.to_str().map(str::to_owned);
          }
      }
      None
  }
  ```
  (`current_exe()` already shows up in the disallowed-methods allowance pattern
  at `maintenance.rs`; gate with `#[allow(clippy::disallowed_methods)]` like
  `path_from_env`.)
- Change `path_from_env(keys, default, tool)` (currently `version.rs:336-341`)
  so that after the env-var `find_map` returns `None`, it tries
  `discover_bundled(tool)` before falling back to `default`. Update the two
  public callers `ffmpeg_path_from_env` (line 325) and `ffprobe_path_from_env`
  (line 331) to pass the tool name (`"ffmpeg"`/`"ffprobe"`).
- No caller changes needed: `command.rs:14`, `subtitles.rs:395`,
  `hls_worker.rs:963/974/1025`, `watermarks.rs:519`, `stream.rs:543`,
  `libraries.rs:181` all route through these two functions and inherit the new
  behavior.

### Scanner resolver (`crates/ersatzrs-scanner/src/main.rs:398`)

`ersatzrs-scanner` does **not** depend on `ersatzrs-ffmpeg`, so do not add a
dependency just to share the resolver. Instead, replicate the same
relative-to-exe probe inside the scanner's private `ffprobe_path_from_env`
(env list already matches at lines 399-404): after the env `find_map` misses,
try `<exedir>/ffmpeg/bin/ffprobe[.exe]` then `<exedir>/ffprobe[.exe]`, else
fall back to `"ffprobe"`.

### Tests (`version.rs`, unix `#[cfg(test)]` module)

Add resolver-precedence tests alongside the existing `unix_tests`:
- env var beats a fabricated bundled binary;
- with no env var set, a fabricated `<tmp>/ffmpeg/bin/ffmpeg` is discovered;
- falls back to `"ffmpeg"` when neither exists.
Note: `discover_bundled` keys off `current_exe()`, which tests can't relocate;
to keep tests deterministic, factor the directory out — give `discover_bundled`
an inner `discover_bundled_in(dir, tool)` taking the base dir, test that, and
have the public path call it with `current_exe().parent()`.

## 2. Build: new `build-ffmpeg` job (`.github/workflows/release.yml`)

Add a job above `binaries`, matrixed over the same 6 RIDs, each producing
artifact `ffmpeg-<rid>` containing a relocatable `ffmpeg/bin/{ffmpeg,ffprobe}[.exe]`
with co-located shared libs (runnable with no `LD_LIBRARY_PATH`/`DYLD_*`).

- **linux-x64 / linux-arm64 / linux-arm** — reuse the Docker recipe (single
  source of truth). Use `docker/setup-qemu-action` + `setup-buildx-action`
  (same as the `docker` job), `docker buildx build --platform linux/<arch>
  --target ffmpeg-export --output type=local,dest=out`, then the payload is at
  `out/ffmpeg`. This requires a new tiny export stage in the Dockerfile (below).
- **win-x64** — `msys2/setup-msys2` with the `mingw-w64-x86_64`
  cairo/libass/x264/x265/svt-av1/etc. stack; build FFmpeg 8.1 from source with
  the same `--enable-*` flags as the Dockerfile; copy `ffmpeg.exe`,
  `ffprobe.exe` and their mingw DLLs into `ffmpeg/bin/` (Windows resolves DLLs
  next to the exe).
- **osx-x64 / osx-arm64** — native runners (`macos-13` for x64,
  `macos-latest`/arm64 for arm64 — x64 must build on an Intel runner so
  Homebrew deps match the arch); Homebrew supplies deps; build with the same
  flags; relocate dylibs to `@loader_path/../lib` via
  `install_name_tool`/`dylibbundler` so no `DYLD_*` is needed.

Every cell runs the **same drawvg smoke test** the Dockerfile uses
(`docker/Dockerfile:161-165`: `-f lavfi -i color=... -vf
"drawvg=script='rect 4 4 16 16 fill',format=yuv420p" -frames:v 1 -f null -`)
against the relocated payload from a clean env **before** `upload-artifact` —
fail the release if drawvg is absent.

## 3. Build: Dockerfile changes (`docker/Dockerfile`)

- Make the built `bin` relocatable: add
  `--extra-ldflags='-Wl,-rpath,$ORIGIN/../lib'` to the `./configure` invocation
  (`Dockerfile:125-155`). Harmless to the runtime image (it still sets
  `LD_LIBRARY_PATH=/opt/ffmpeg/lib`) and makes the extracted tree self-locating.
- Add an export-only stage so buildx can emit just the payload:
  ```dockerfile
  FROM scratch AS ffmpeg-export
  COPY --from=ffmpeg-build /opt/ffmpeg /ffmpeg
  ```
  Place it after the `ffmpeg-build` stage. It is not referenced by the default
  `runtime` target, so the published image is unaffected.

## 4. Consume payload in `binaries` job (`.github/workflows/release.yml`)

- Add `build-ffmpeg` to `needs` (currently `needs: release` at line 26 → `needs:
  [release, build-ffmpeg]`).
- Before/within "Stage archive" (lines 88-115), `actions/download-artifact` the
  matching `ffmpeg-${{ matrix.rid }}` and `cp -R` its `ffmpeg/` tree into
  `$stage/ffmpeg/` (works for both the tar.gz and zip branches).
- Result per archive: `ersatzrs-web[.exe]`, `ersatzrs-scanner[.exe]`, `assets/`,
  `ffmpeg/bin/...` (+ libs), `LICENSE`, `README.md`.

## 5. Docs

- `BUILD.md` / `RELEASING.md`: note that release archives now ship a bundled
  cairo-enabled FFmpeg 8.1 and the discovery order (env override > bundled >
  PATH).
- `STATUS.md`: add a session-log row and tick the relevant Phase 7 hardening
  item (this is parity/UX hardening). No health-check change — bundling does not
  alter FFmpeg capabilities, and the existing capability check already covers
  them.

## Out of scope

DB-config FFmpeg path (`ffmpeg.ffmpeg_path` stored by the settings page is not
read by runtime resolution today). That is a pre-existing gap, not part of
bundling.

## Critical files

- `crates/ersatzrs-ffmpeg/src/version.rs:325-341` — `discover_bundled` + new
  resolution order + unit tests.
- `crates/ersatzrs-scanner/src/main.rs:398-409` — replicate relative-probe.
- `.github/workflows/release.yml` — new `build-ffmpeg` job + `binaries`
  `needs`/staging change.
- `docker/Dockerfile:125-155` — rpath flag; new `ffmpeg-export` stage.
- `BUILD.md`, `RELEASING.md`, `STATUS.md` — docs.

## Verification

- **Unit**: `cargo test -p ersatzrs-ffmpeg` — resolver precedence tests pass.
- **Local relocatability smoke (linux-x64)**:
  `docker buildx build --platform linux/amd64 --target ffmpeg-export
  --output type=local,dest=out docker/`-context build; place `out/ffmpeg` next
  to a built `ersatzrs-web`; run from a clean env (no `ErsatzRS_FFMPEG__PATH`,
  no `LD_LIBRARY_PATH`, FFmpeg off `PATH`) and confirm the startup capability
  check passes and `drawvg` works.
- **CI**: run the workflow on a throwaway tag; confirm each of the 6 archives
  contains `ffmpeg/bin/{ffmpeg,ffprobe}` and that each cell's drawvg smoke test
  passed. Download win-x64 + osx archives and launch on a host with no FFmpeg to
  confirm self-containment.
- **Regression gates stay green**: `cargo fmt --all -- --check`; pipeline
  `insta` snapshots; clippy per repo convention — `cargo clippy --workspace
  --all-targets -- -D warnings` plus a separate `--features mysql` pass.