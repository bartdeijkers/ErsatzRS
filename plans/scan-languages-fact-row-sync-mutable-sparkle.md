# Embedded subtitle extraction on deep show scan (close the last deferral)

Status: planned. Created 2026-05-21. Executable by the
`leptos-ersatztv-porter` agent (mostly infra/scanner; no UI).

## Context

The TV-show-page parity work (`plans/scan-languages-fact-row-sync.md`) closed
its three gaps but left one documented deferral: ErsatzTV queues
`ExtractEmbeddedShowSubtitles` after a **successful deep show scan**; ErsatzRS
only logs a debug line at the dispatch site. This plan ports that behaviour
faithfully so the show-scan Scan(Deep) action extracts embedded text subtitles
and font attachments exactly as ErsatzTV does, then removes the deferral notes.

**Scope (confirmed with maintainer):** the show-scan path only — *not* the
playout-window `ExtractEmbeddedSubtitles` (BuildPlayout / channel / settings
triggers) nor transcode-time subtitle burn-in. Those stay separate, documented
gaps. Enablement is **config-key only**, matching ErsatzTV's default
(`extract_embedded_subtitles = false`): the code path is fully ported but
dormant until the key is set; ErsatzRS has no global FFmpeg settings page yet,
which is recorded as its own parity gap (out of scope here).

### Two findings that shape the work

1. **`Subtitle` table is never populated during scan.** ErsatzRS providers
   write only `MediaStream` rows (`MediaStreamKind::Subtitle/ExternalSubtitle`);
   there is no `UpdateSubtitles` equivalent. ErsatzTV's extraction query reads
   the `Subtitle` table (`SUB.SubtitleKind=0 AND (SUB.IsExtracted=0 OR
   SUB.Path IS NULL)`). Without first porting `Subtitle`-row population,
   extraction is a permanent no-op. **This prerequisite is Slice 1.**
2. **No media proxy needed.** ErsatzTV's `GetMediaItemPath` builds internal
   streaming-proxy URLs for Plex/Jellyfin/Emby. ErsatzRS instead feeds ffmpeg
   the `MediaFile."Path"` with path-replacements — see
   [stream.rs `load_active_media_path` / `load_path_replacements`](../../ErsatzRS/crates/ersatzrs-web/src/transport/iptv/stream.rs#L111).
   Extraction reuses that same resolution. This is the existing ErsatzRS
   input-resolution mechanism (not a new deviation); cite it in a comment.

## Authoritative C# sources

- Dispatch after deep scan: [`QueueShowScanByLibraryIdHandler.cs`](../../ErsatzTV/ErsatzTV.Application/Libraries/Commands/QueueShowScanByLibraryIdHandler.cs) (lines 92–95: `if (success && request.DeepScan) → ExtractEmbeddedShowSubtitles(ShowId)`).
- Show handler + gating: [`ExtractEmbeddedShowSubtitlesHandler.cs`](../../ErsatzTV/ErsatzTV.Application/Subtitles/Commands/ExtractEmbeddedShowSubtitlesHandler.cs) — gate on `FFmpegUseEmbeddedSubtitles` (default true) **and** `FFmpegExtractEmbeddedSubtitles` (default false); per-show episode query; loop `ExtractSubtitles`+`ExtractFonts`.
- Extraction engine: [`ExtractEmbeddedSubtitlesHandlerBase.cs`](../../ErsatzTV/ErsatzTV.Application/Subtitles/Commands/ExtractEmbeddedSubtitlesHandlerBase.cs) — `GetMediaItemIdsWithTextSubtitles` (image-codec filter), `ExtractSubtitles` (ffmpeg argv + DB update), `ExtractFonts` (`-dump_attachment`), `GetRelativeOutputPath` (MD5 hashing + extension map).
- Row population: `Subtitle.FromMediaStream` ([`Subtitle.cs`](../../ErsatzTV/ErsatzTV.Core/Domain/Metadata/Subtitle.cs)) + `MetadataRepository.UpdateSubtitles` ([`MetadataRepository.cs`](../../ErsatzTV/ErsatzTV.Infrastructure/Data/Repositories/MetadataRepository.cs#L626)) + scan trigger `MediaServerTelevisionLibraryScanner.UpdateSubtitles` ([scanner](../../ErsatzTV/ErsatzTV.Scanner/Core/Metadata/MediaServerTelevisionLibraryScanner.cs#L832)).
- Config keys: [`ConfigElementKey.cs`](../../ErsatzTV/ErsatzTV.Core/Domain/ConfigElementKey.cs) (`ffmpeg.use_embedded_subtitles`, `ffmpeg.extract_embedded_subtitles`).
- Cache layout: [`FileSystemLayout.cs`](../../ErsatzTV/ErsatzTV.Core/FileSystemLayout.cs) (`SubtitleCacheFolder`, `FontsCacheFolder`).

## Conventions (per `plans/scan-languages-fact-row-sync.md`)

- PORTING.md: read the C# first; map idioms (MediatR handler → async fn,
  LanguageExt → `Result`). Mechanism maps get a code comment citing the C#
  path. Two **deliberate divergences** here, both consistent with existing
  ErsatzRS patterns (so a one-line comment, no ADR): input path via
  `MediaFile."Path"`+replacements instead of proxy URLs; ffmpeg path via
  `ersatzrs_ffmpeg::ffmpeg_path_from_env()` instead of the `FFmpegPath` config
  key. If unsure whether either rises to an ADR, ask before coding.
- ARCHITECTURE.md: SQL/provider code in `ersatzrs-infra`; HTTP/server-fn in
  `ersatzrs-web`. No cycles.
- STYLE.md: quoted `PascalCase` SQL identifiers, `PK_/FK_/IX_` names, preserve
  ErsatzTV JSON/casing, `thiserror`/`anyhow` split.
- TEST.md: provider/SQL ⇒ integration tests under `crates/<crate>/tests/`;
  pure logic ⇒ `#[cfg(test)]`; snapshot ffmpeg argv with `insta`.
- **Gates per slice** (clippy-CI memory): `cargo fmt --all`;
  `cargo clippy --workspace --all-targets -- -D warnings` **and**
  `cargo clippy -p ersatzrs-web -p ersatzrs-infra --all-targets
  --features ersatzrs-web/mysql -- -D warnings`; `cargo test --workspace`.
  ffmpeg-touching integration tests need
  `LD_LIBRARY_PATH=/home/bart/.local/ffmpeg-8.1/lib`.
- Session handoff: each slice prepends a STATUS.md session-log row + updates
  "What an Agent Should Do Next"; update `docs/reference/parity-matrix.md` when
  the gap closes. Commit per slice (Conventional Commits) **only when asked**.

---

## Slice 1 — Populate `Subtitle` rows during episode scan (prerequisite)

Port `Subtitle.FromMediaStream` + `MetadataRepository.UpdateSubtitles` for
**episodes** (TV) so deep show scans persist `Subtitle` rows the extractor can
query. Add/remove/update keyed on `(EpisodeMetadataId, StreamIndex)`; reset
`IsExtracted=0` when `SubtitleKind`/`Codec`/`Language`/flags
(`Default`/`Forced`/`SDH`) change or the path changes (mirror
`MetadataRepository.UpdateSubtitles` lines 666–717). Map `MediaStreamKind`:
`Subtitle → SubtitleKind::Embedded (0)`, `ExternalSubtitle → Sidecar (1)`.

- New helper, e.g. `pub async fn update_episode_subtitles(tx, episode_metadata_id, streams)` in [`crates/ersatzrs-infra/src/repositories/metadata.rs`](../../ErsatzRS/crates/ersatzrs-infra/src/repositories/metadata.rs) (mirrors `MetadataRepository.UpdateSubtitles`). Source from the just-written subtitle/external-subtitle `MediaStream`s for the episode's media version.
- Call it from each provider's per-show episode path, right after `replace_streams`: [`plex.rs replace_streams`](../../ErsatzRS/crates/ersatzrs-infra/src/plex.rs#L1945), [`jellyfin.rs:2952`](../../ErsatzRS/crates/ersatzrs-infra/src/jellyfin.rs#L2952), [`emby.rs:2853`](../../ErsatzRS/crates/ersatzrs-infra/src/emby.rs#L2853). Because `replace_streams` is shared by full-library and single-show sync, row population applies to both (matches ErsatzTV, whose `UpdateSubtitles` runs in the shared scanner) — confirm no behaviour regression in existing provider snapshot tests.
- Gate row refresh like ErsatzTV: always on `deep_scan`/added/etag-changed (the episode path is only reached when the item is being (re)synced, so populate whenever streams are rewritten).

**Tests:** extend `crates/ersatzrs-infra/tests/{plex,jellyfin,emby}_service.rs`
(or a metadata-repo test): seed an episode whose ffprobe/provider payload has
embedded text + image subtitle streams; assert `Subtitle` rows created with
correct `SubtitleKind`/`Codec`/`StreamIndex`/`IsExtracted=0`; re-run with a
changed codec → `IsExtracted` reset; removed stream → row deleted.

---

## Slice 2 — Config keys + cache-folder layout

- Add key constants `ffmpeg.use_embedded_subtitles` and
  `ffmpeg.extract_embedded_subtitles` (place near other infra config-key
  constants; consumed via the existing [`ConfigRepository`](../../ErsatzRS/crates/ersatzrs-infra/src/repositories/config.rs) `get`/`set`). Helpers returning ErsatzTV defaults: `use_embedded_subtitles` → true, `extract_embedded_subtitles` → false (string `"True"/"False"` or `"1"/"0"` — match how existing bool config values are encoded; verify against current `ConfigElement` usage).
- Add a small `FileSystemLayout`-style helper (new module, e.g.
  `crates/ersatzrs-infra/src/file_system_layout.rs`) exposing
  `subtitle_cache_folder()` and `fonts_cache_folder()` =
  `<cache_root>/cache/streams/{subtitles,fonts}`. Pick `<cache_root>` to match
  existing ErsatzRS convention — the logs base is `data_local_dir()/ErsatzRS`
  ([`logging.rs:137`](../../ErsatzRS/crates/ersatzrs-infra/src/logging.rs#L137)); reuse that base for consistency. Cite `FileSystemLayout.cs`.

**Tests:** `#[cfg(test)]` unit asserting the folder paths end with
`cache/streams/subtitles` / `.../fonts`, and config defaults resolve to
true/false when the key is absent.

---

## Slice 3 — Subtitle + font extraction engine

New module `crates/ersatzrs-infra/src/subtitles.rs` porting
`ExtractEmbeddedSubtitlesHandlerBase` + `ExtractEmbeddedShowSubtitlesHandler`,
scoped to episodes.

- `extract_embedded_show_subtitles(pool, show_id)`:
  1. Resolve & verify ffmpeg path via `ersatzrs_ffmpeg::ffmpeg_path_from_env()` (divergence comment vs `FFmpegPathMustExist`).
  2. Gate: return early unless `use_embedded_subtitles == true` **and** `extract_embedded_subtitles == true` (mirror handler lines 48–64).
  3. Episode query (port lines 75–85): episodes under the show with an embedded `Subtitle` where `IsExtracted=0 OR Path IS NULL`.
  4. Filter to text subtitles (`GetMediaItemIdsWithTextSubtitles`): exclude codecs `hdmv_pgs_subtitle, dvd_subtitle, dvdsub, vobsub, pgssub, pgs`.
  5. Per episode: `extract_subtitles` then `extract_fonts`.
- `extract_subtitles`: build relative output path via the **MD5** scheme
  (`GetRelativeOutputPath`): `MD5_hex_upper("{mediaItemId}_{streamIndex}_{codec}")`, subfolders `[..2]/[2..4]`, extension map `subrip|srt→.srt, ass→.ass, webvtt→.vtt, mov_text→.srt` (others skipped). Add the `md-5` crate if absent (note the dep add). Delete any existing output, then one ffmpeg run: `-nostdin -hide_banner -i <input>` then per subtitle `-map 0:<StreamIndex> -c:s <copy|text> <fullOutputPath>` (`text` only for `mov_text`). On exit 0, `UPDATE "Subtitle" SET "IsExtracted"=1, "Path"=<relative> WHERE "Id"=?`.
- `extract_fonts`: for each `Attachment` stream whose `MimeType` contains `font`/`opentype`, skip if `<FontsCacheFolder>/<FileName>` exists, else run `-nostdin -hide_banner -dump_attachment:t:<attachmentIndex> "" -i <input> -y` with cwd = fonts cache; success = file exists (ffmpeg exits 1 even on success).
- `<input>` = episode head-version `MediaFile."Path"` with path-replacements applied — reuse the resolution behind [stream.rs `load_path_replacements`](../../ErsatzRS/crates/ersatzrs-web/src/transport/iptv/stream.rs#L152) (extract a shared fn if it currently lives in `ersatzrs-web`; otherwise port the same logic into infra). Comment the divergence from ErsatzTV proxy URLs.
- Spawn ffmpeg with `tokio::process::Command` (pattern as [`ffprobe.rs`](../../ErsatzRS/crates/ersatzrs-infra/src/ffprobe.rs#L72) / [`hls_worker.rs`](../../ErsatzRS/crates/ersatzrs-infra/src/hls_worker.rs#L101)), capturing stderr for the failure log.

**Tests:** pure-logic `#[cfg(test)]`: `GetRelativeOutputPath` MD5/extension
cases; image-codec filter; ffmpeg argv builder (snapshot with `insta`).
Optional ignored integration test extracting from a tiny fixture mkv (needs
`LD_LIBRARY_PATH`) — only if a fixture is cheap; otherwise rely on argv
snapshot + Slice 4 wiring test.

---

## Slice 4 — Wire into deep show scan + close the deferral

- In [`scan_show`](../../ErsatzRS/crates/ersatzrs-web/src/admin/server_fns/libraries.rs#L259), replace the deferral debug-log (lines 259–268) with: after a **successful** deep sync, call `subtitles::extract_embedded_show_subtitles(pool, show_id)` (the fn self-gates on the config settings, so default-false stays a no-op). Mirror `QueueShowScanByLibraryIdHandler` (success && deepScan). Runs inside the already-spawned background task, after sync, before unlock/clear-progress.
- Remove the deferral module-doc lines in [`shows.rs:23–25`](../../ErsatzRS/crates/ersatzrs-web/src/admin/pages/media/shows.rs#L23).
- Update `docs/reference/parity-matrix.md` (line ~35): change the deferred note to "ported, gated by `ffmpeg.extract_embedded_subtitles` (default off)"; add the remaining gaps (playout-window extraction, transcode consumption, global FFmpeg settings UI) as the new documented deferrals.
- Update `STATUS.md` parity note (lines ~217–221) accordingly.
- Update `plans/scan-languages-fact-row-sync.md` line 217–221 and reference this plan; index this plan in `plans/INDEX.md`.

**Tests:** a web/infra test that with both config keys true a deep `scan_show`
reaches extraction (assert via a seeded extractable subtitle row flipping to
`IsExtracted=1`, or that the extract entry is invoked), and that with
`extract_embedded_subtitles=false` it is a no-op.

---

## End-to-end verification

1. Per slice: the gate commands (fmt, both clippy passes, `cargo test --workspace`); ffmpeg tests with `LD_LIBRARY_PATH=/home/bart/.local/ffmpeg-8.1/lib`.
2. Manual: set `ffmpeg.extract_embedded_subtitles=true` in `ConfigElement`; point at a Plex/Jellyfin/Emby show whose episodes carry embedded text subtitles (e.g. `subrip`/`ass`) and font attachments; Deep-Scan from the show page; confirm `Subtitle` rows gain `IsExtracted=1` + `Path`, and `<cache>/cache/streams/subtitles/ab/cd/<hash>.srt` and `.../fonts/<font>` files appear. With the key false (default), confirm Deep-Scan extracts nothing.
3. Sandbox note: long-lived servers get killed — prefer a `timeout`-bounded scanner/web boot or defer the live UI check to the maintainer.

## Out of scope / remaining documented deferrals after this plan

- Playout-window `ExtractEmbeddedSubtitles` (BuildPlayout / channel / FFmpeg-settings triggers).
- Consuming extracted subtitles in the transcode pipeline (burn-in / sidecar mux).
- Global FFmpeg settings UI page (so the toggle has no UI yet — config-key only).
- Subtitle-row population for movies / music videos / other videos (only episodes here).
