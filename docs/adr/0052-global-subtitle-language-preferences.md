# ADR 0052: Add global subtitle language preferences

## Status

Accepted (2026-07-22)

## Implementation

Implemented (2026-07-22) — persisted settings, localized admin controls,
shared MPEG-TS/HLS language resolution, fallback-aware subtitle selection,
health coverage, and focused Rust/browser/runtime regression proof are in
place.

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

Built-in playback resolves language preferences as follows:

- Audio uses playout-item preference, then channel preference, then the global
  audio default. ErsatzTV's HLS Direct exception remains intact: without an
  explicit item or channel audio language or title, all source audio streams
  remain available.
- Subtitle primary language uses playout-item preference, then channel
  preference, then the global subtitle default.
- If built-in selection cannot find a subtitle satisfying the channel subtitle
  mode in the primary language, it tries the global fallback language. The
  fallback also applies when the primary came from a playout item or channel.
- ISO-639 aliases are treated as the same language. An equivalent primary and
  fallback are searched only once.
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
defaults.

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
