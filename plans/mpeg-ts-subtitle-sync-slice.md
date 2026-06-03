# Fix MPEG-TS burned-subtitle timing drift (port ErsatzTV's seek re-basing)

## Context

In MPEG-TS playback, burned (text) subtitles display out of sync with the
video. Reproduced with Star Trek: TNG Dutch subtitles, which have verified-correct
timings in the source files — so the drift is introduced by ErsatzRS, not the media.

**Root cause.** When a client joins a channel mid-program, ErsatzRS seeks into the
current item. The seek is emitted as a single global `-ss` placed immediately before
the first `-i` (the video) and **without `-copyts`**, so FFmpeg resets the video PTS
to start at 0 at the seek point:

- `crates/ersatzrs-ffmpeg/src/command.rs:47-51` — `-ss <seconds>` emitted once, before the input loop (applies to the video input only).
- No `CopyTimestamp`/`-copyts` is ever emitted (it does not exist in the pipeline).

Meanwhile, burned **text** subtitles are read from the cached extracted file
(`subtitle_cache_folder()/<hash>.srt|ass|vtt`) and burned via the `subtitles=<path>`
filter, which renders according to the file's **absolute** timestamps (from 00:00):

- `crates/ersatzrs-infra/src/playback.rs:459-461` / `subtitle_text_path` (463-487) — burned text subtitle resolves to the cached file path.
- `crates/ersatzrs-ffmpeg/src/pipeline/mod.rs:689-698` — pushes `subtitles_filter(&input.path, …)` onto the video chain.
- `crates/ersatzrs-ffmpeg/src/filter/simple.rs:209-219` — `subtitles=<file>:fontsdir=<dir>`; no timing offset.

So at a join offset of N seconds, the video frame truly at minute N is shown with
PTS 0, but the subtitles filter overlays the subtitle authored for time 0 → subtitles
appear **shifted earlier by the seek amount**. At a 0s offset everything lines up;
the drift grows with the join offset, matching the reported symptom.

**Not affected:** image-based subtitles (PGS/VOBSUB). Their input uses the same path
as the video, so `command.rs` path-dedup (`command.rs:73-85`) reuses the seeked video
`-i`, and `input_index` (`filter/complex.rs:274-286`) resolves the overlay to input 0
— they ride the seeked input and stay in sync. Confirmed scope: **burned text subtitles only.**

## How ErsatzTV solves it

ErsatzTV runs a **separate ffmpeg process** to re-base the subtitle to the seek point
*before* burning, instead of using `-copyts` on the video:

- `ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs:78-102` `Seek(inputFile, codec, seek)` builds: `-nostdin -hide_banner -nostats -loglevel error -i <sub> -ss <seek> -c:s copy -f <ass|webvtt|srt> pipe:1`.
- The seek uses `StreamSeekFilterOption` (`GlobalOption/StreamSeekFilterOption.cs`) whose `-ss` is a **filter/output-side** option emitted *after* `-i` → an output seek that drops earlier events and **re-bases the remaining subtitle timestamps to 0**.
- Wired via `FFmpegLibraryProcessService.SeekTextSubtitle` (line 1158) → `GetSeekTextSubtitleProcessHandler`; the re-based stream is served over an HTTP localhost endpoint and burned via `subtitles=http://localhost:…` (`Filter/SubtitlesFilter.cs:44`).
- `CopyTimestampInputOption` exists but is **unused** — ETV deliberately re-bases the subtitle rather than preserving absolute video PTS. The main video pipeline keeps input-seek-to-0 (`StreamSeekInputOption.cs`); both video and subtitle then start at 0 → in sync.

ErsatzRS already ports the *extraction* half of this (`crates/ersatzrs-infra/src/subtitles.rs`) but is **missing the playback-time re-basing pass**.

## Approach (confirmed with maintainer)

Port ErsatzTV's `SeekTextSubtitle` re-basing as a **separate ffmpeg pass that writes a
time-shifted temp subtitle file** (chosen over an HTTP endpoint — simpler, single-binary
friendly, consistent with ErsatzRS's existing subtitle file-cache model in `subtitles.rs`).
Scope: **burned text subtitles only**.

When `start_seconds > 0` and the selected subtitle is a burned text subtitle, re-base the
cached/sidecar subtitle file by `start_seconds` and burn the re-based temp file instead of
the original.

## Implementation

**1. New re-base helper — `crates/ersatzrs-infra/src/subtitles.rs`** (home of the existing
extraction port; both callers already import from `ersatzrs-infra::playback`/`subtitles`).

- Add `build_seek_subtitle_args(input_path, seek_seconds, out_path) -> Vec<String>` mirroring `PipelineBuilderBase.Seek`:
  `-nostdin -hide_banner -nostats -loglevel error -i <input> -ss <hh:mm:ss> -c:s copy -f <ass|webvtt|srt> <out_path>`.
  Derive the output format/extension from the input file extension (`.ass`→`ass`, `.vtt`→`webvtt`, else `srt`) — reuse the same extension map already encoded in `relative_output_path` (`subtitles.rs:57-63`). Format the seek as `HH:MM:SS` to match `{_start:c}`.
- Add `async fn rebase_text_subtitle(input_path: &str, seek_seconds: i64) -> Option<String>`: returns the original path unchanged when `seek_seconds <= 0`; otherwise spawns ffmpeg (`ffmpeg_path_from_env()`, as elsewhere in this file), writes to a deterministic temp path under a new `seek` subdir of `subtitle_cache_folder()` keyed by `MD5(input_path)` + `_<seconds>` + ext, and returns that path. On spawn/exit failure, log and fall back to the original path (degrade to current behavior, never break playback).

**2. Wire into the two command builders** — both receive `start_seconds` and the subtitle `InputFile`:

- `crates/ersatzrs-infra/src/hls_worker.rs:170-193` (`build_hls_command` call site).
- `crates/ersatzrs-web/src/transport/iptv/stream.rs:125-137` (`build_stream_command` call site).

In each, after `subtitle_input_for_stream(...)` returns the input, if it is a **burned, non-image** text subtitle and `start_seconds > 0`, call `rebase_text_subtitle(&input.path, start_seconds)` and swap the input's `path` to the returned temp file. Keep this branch out of the FFmpeg crate so existing pipeline snapshots are untouched. (Optionally expose a small `InputFile` helper to identify burned text subtitles, or match on `kind == Subtitle && !is_image_based_subtitle && subtitle_method == Some(Burn)`.)

**3. No change to the FFmpeg pipeline argv.** The main playback command keeps input-seek-to-0
for video; only the burned subtitle's file path changes to the re-based copy, so the
`subtitles=` filter now reads timestamps that also start at 0 → in sync.

## Files

- `crates/ersatzrs-infra/src/subtitles.rs` — new `build_seek_subtitle_args` + `rebase_text_subtitle` (+ unit/snapshot test, mirroring `build_subtitle_args_snapshot`).
- `crates/ersatzrs-infra/src/file_system_layout.rs` — add a `seek` subfolder helper under the subtitle cache (reuse `subtitle_cache_folder`).
- `crates/ersatzrs-infra/src/hls_worker.rs` — re-base before `build_hls_command`.
- `crates/ersatzrs-web/src/transport/iptv/stream.rs` — re-base before `build_stream_command`.
- Reference (read-only): `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs:78-102`, `GlobalOption/StreamSeekFilterOption.cs`, `ErsatzTV.Core/FFmpeg/FFmpegLibraryProcessService.cs:1158-1202`.

## Verification

1. **Unit/snapshot test** in `subtitles.rs`: assert `build_seek_subtitle_args` produces the expected argv for `.srt`/`.ass`/`.vtt` inputs and a sample seek (e.g. 900s → `00:15:00`), and that `seek_seconds <= 0` skips re-basing.
2. **Lint/test** per CLAUDE.md + memory: `cargo clippy --workspace --all-targets -- -D warnings` *and* a `--features mysql` pass; `cargo test --workspace`; `cargo insta review` if snapshots change.
3. **End-to-end (manual)** with the TNG Dutch-subtitle item:
   - Run `cargo run --bin ersatzrs-web`; play an MPEG-TS channel and **join mid-episode** (non-zero `start_seconds`).
   - Confirm a re-based file appears under `subtitle_cache_folder()/seek/…` and the burn command's `subtitles=` path points to it.
   - Verify subtitles now match the spoken Dutch dialogue at the join point (previously shifted early by the join offset). Spot-check a join at 0s (no drift before, still correct) and a large offset (e.g. 20+ min).
4. **Regression**: confirm image-subtitle (PGS) playback still works and existing FFmpeg pipeline snapshots are unchanged (no argv change for the main process).

## Risks / notes

- Output seek (`-ss` after `-i`) re-bases by **dropping** pre-seek events; correct for joining mid-item. The re-based file is valid for one item from its seek point — regenerated per item (matches the per-item transcode model in `hls_worker`/`stream`).
- Temp-file delivery is a deliberate, documented divergence from ErsatzTV's HTTP-pipe (note it in the `subtitles.rs` module doc alongside the existing divergence notes).
- Failure of the re-base pass falls back to the un-shifted file (today's behavior) rather than failing playback.
