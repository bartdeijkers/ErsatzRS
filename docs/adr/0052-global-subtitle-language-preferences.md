# ADR 0052: Add global subtitle language preferences

## Status

Accepted (2026-07-22)

## Implementation

Implemented (2026-07-22), amended (2026-07-30) — persisted settings,
localized admin controls, shared MPEG-TS/HLS language resolution,
fallback-aware subtitle selection, Kodi-style playback-language options,
truthful `und` handling, health coverage, and focused Rust/browser/runtime
regression proof are in place.

## Context

ErsatzTV exposes `ffmpeg.preferred_language_code` as the default audio
language. Its built-in audio selector reads that value when neither a playout
item nor a channel specifies an audio language. Subtitle selection has no
equivalent global setting: it only considers the playout-item or channel
preference and otherwise ranks every eligible subtitle by the channel subtitle
mode.

ErsatzRS already displays and persists the default audio language, but its
MPEG-TS and HLS playback paths do not currently consume the setting. Operators
also need a global primary subtitle language and a second language to try when
the primary is unavailable. Requiring every channel or schedule item to repeat
those preferences is error-prone and makes consistent multilingual playback
difficult.

## Decision

Deliberately extend ErsatzTV with two optional `ConfigElement` values:

- `ffmpeg.preferred_subtitle_language_code`
- `ffmpeg.fallback_subtitle_language_code`

Both values default to unset so upgrading or initializing ErsatzRS does not
change subtitle selection until an operator opts in. `/settings/ffmpeg`
exposes localized default and fallback subtitle language selectors.

The audio and subtitle selectors expose Kodi's end-user language vocabulary:
languages with an ISO-639-1 code, persisted through ErsatzRS's existing
canonical three-letter codes. This excludes historical and specialist
ISO-639-3-only entries that are not useful playback preferences. Existing
saved values outside the curated list remain visible in their owning selector
until the operator changes them, preventing an unrelated settings save from
silently replacing the value. Known codes are matched case-insensitively after
trimming surrounding whitespace and rendered with their canonical persisted
code, so legacy values such as `ENG` or ` eng ` retain their semantic
selection instead of falling back to `und` or no subtitle.

`und` is the single special persisted language value. The audio selector labels
it `Original (und)` and uses it to prefer an undefined-language source track,
then the media item's overall default track when no undefined track exists.
The subtitle selectors label it `Undefined (und)` and match undefined-language
subtitles. Labels are localized in every supported admin culture.

Built-in playback resolves language preferences as follows:

- Audio uses playout-item preference, then channel preference, then the global
  audio default. ErsatzTV's HLS Direct exception remains intact: without an
  explicit item or channel audio language or title, all source audio streams
  remain available, except that a global `und` value explicitly requests the
  original/default source track.
- Subtitle primary language uses playout-item preference, then channel
  preference, then the global subtitle default.
- If built-in selection cannot find a subtitle satisfying the channel subtitle
  mode in the primary language, it tries the global fallback language. The
  fallback also applies when the primary came from a playout item or channel.
- ISO-639 aliases are treated as the same language. An equivalent primary and
  fallback are searched only once.
- A null, blank, or literal `und` stream language is treated as `und` by the
  built-in selectors, matching the custom stream-selector contract. Local
  sidecar subtitles without a language suffix are persisted as `und`, matching
  ErsatzTV commit `32e5482`; existing null rows work immediately and normalize
  on their next scan.
- With neither subtitle language configured, selection retains ErsatzTV's
  unfiltered ranking. When a configured primary and fallback both fail,
  playback selects no subtitle rather than an unrelated language.

`ChannelSubtitleMode::None`, generated music-video credit subtitles, custom
stream selectors, and explicit troubleshooting choices keep their existing
precedence.

## Consequences

Operators can define a consistent subtitle preference once while retaining
item- and channel-level overrides. The fallback makes bilingual libraries
predictable without silently selecting an arbitrary third language.

The new keys are additive `ConfigElement` rows and require no database schema
migration. ErsatzTV ignores them when an ErsatzRS database is inspected or
migrated. Playback must treat configuration read failures as operational
errors; it must not silently replace unreadable persisted preferences with
defaults. The `und` scanner correction also requires no migration: the runtime
accepts existing null metadata and local rescans persist the normalized tag.

## Alternatives Considered

- Add only a primary subtitle language. Rejected because it does not support
  libraries where the preferred language is inconsistently available.
- Fall back to any eligible subtitle after both configured languages fail.
  Rejected because it makes the language preference advisory and can enable an
  unexpected subtitle language.
- Apply the fallback only after the global primary. Rejected because explicit
  item and channel preferences also need the operator's configured recovery
  language when their preferred track is absent.
- Seed English for new installations. Rejected because it would change current
  subtitle behavior before the operator chooses a preference.

## References

- `$ERSATZTV_SRC/ErsatzTV/Pages/Settings/FFmpegSettings.razor`
- `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegStreamSelector.cs`
- `$ERSATZTV_SRC/ErsatzTV.Core/Domain/ConfigElementKey.cs`
- `$ERSATZTV_SRC` commit
  `32e548285acc38a5f1cc202d481163d345c99a19`
- `xbmc/xbmc` commit `677e9e93fe32fa7c63f7c99b9681a79811c74627`,
  `system/settings/settings.xml` and `xbmc/LangInfo.cpp`
