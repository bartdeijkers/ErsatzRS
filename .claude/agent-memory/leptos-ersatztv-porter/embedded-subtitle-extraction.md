---
name: embedded-subtitle-extraction
description: How ExtractEmbeddedShowSubtitles was ported to ersatzrs-infra, and the Plex Subtitle-row blocker.
metadata:
  type: project
---

`ExtractEmbeddedShowSubtitles` (deep-show-scan subtitle/font extraction) is
ported in `ersatzrs-infra`, gated by `ffmpeg.extract_embedded_subtitles`
(default off) + `ffmpeg.use_embedded_subtitles` (default on). See
[[per-show-targeted-sync]] for the scan_show dispatch this hangs off.

**Pieces:**
- `metadata::update_episode_subtitles(tx, episode_metadata_id, streams)` — ports
  `Subtitle.FromMediaStream` + `MetadataRepository.UpdateSubtitles` (diff on
  `(EpisodeMetadataId, StreamIndex)`; `IsExtracted` reset on
  kind/codec/language/flag/path change). `SDH` always false (C# factory doesn't
  set it). Lives in `src/metadata.rs` (NOT `repositories/metadata.rs` — that's
  the trait impl with no tx-based free fns; providers call `crate::metadata::*`).
- `ffmpeg_config` module — bool config keys; encode "True"/"False", parse with
  `eq_ignore_ascii_case("true") || "1"` (same pattern as `health.rs`
  `ffmpeg.save_reports`).
- `file_system_layout` module — `cache/streams/{subtitles,fonts}` under
  `data_local_dir()/ErsatzRS` (same base as `logging::default_log_dir`).
- `subtitles::extract_embedded_show_subtitles(pool, show_id)` — the engine.
  MD5 upper-hex output paths, `subfolder=[..2]/[2..4]`, codec→ext map, image
  codecs excluded, `-map 0:idx -c:s copy|text`, fonts via `-dump_attachment:t:i`
  (0-based position among Attachment streams, NOT stream index; success = file
  exists since ffmpeg exits 1). `md-5` crate added.

**Two deliberate divergences (commented in code, no ADR):** input path is
`MediaFile."Path"` + path-replacements (queried directly in infra via UNION of
the three `*PathReplacement` tables — infra can't depend on ersatzrs-web's
stream.rs); ffmpeg path via `ersatzrs_ffmpeg::ffmpeg_path_from_env()`.

**Plex blocker (documented deferral, not dropped):** Plex scanner writes NO
EpisodeMetadata rows, and `Subtitle` FKs to EpisodeMetadata, so Plex subtitle
rows can't exist. `update_episode_subtitles` is wired into Jellyfin/Emby only.
Extraction self-gates + INNER JOINs EpisodeMetadata, so it naturally no-ops for
Plex. To finish Plex: port Plex EpisodeMetadata population first.

**Wiring:** `scan_show` in `libraries.rs` calls the engine after a successful
deep sync (mirrors `QueueShowScanByLibraryIdHandler` success && deepScan).

**Testing notes:** the full extract-and-flip-to-IsExtracted=1 path writes to the
real `data_local_dir()` cache (no override hook), so CI tests cover only the
config gate + empty-work early return; the flip is a manual verification step.
ffmpeg-touching tests need `LD_LIBRARY_PATH` pointed at the FFmpeg 8.1 `lib`
directory.
Provider show snapshot tests assert `StreamCount` — adding subtitle streams to a
mock bumps it (had to hand-edit two `.snap` files; cargo-insta is NOT installed).
