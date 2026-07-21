# ADR 0049: Name FFmpeg profile audio transcoding explicitly

## Status

Accepted (2026-07-21)

## Implementation

Implemented (2026-07-21) — paired SQLite/MySQL migrations rename the column;
the domain, persistence, server-function, and localized admin UI contracts now
use `TranscodeAudio` / `transcode_audio`; migration and profile-editor
regressions cover the final schema and copy-audio behavior.

## Context

ErsatzTV stores an `FFmpegProfile.NormalizeAudio` boolean and labels its
editor checkbox "Normalize Audio". That wording is ambiguous: it does not
enable loudness normalization. The upstream create/update handlers instead use
the boolean as a save-time gate: when it is false, they persist
`AudioFormat = Copy`; when true, they persist the selected audio codec.
Playback then acts on `AudioFormat`, while the separate
`NormalizeLoudnessMode` controls FFmpeg's `loudnorm` filter.

The inherited name obscures the operational choice available to ErsatzRS
admins: transcode audio to the profile settings, or pass the source audio
through. It is particularly misleading beside the distinct LoudNorm control.

## Decision

Deliberately diverge from ErsatzTV terminology and call this setting
`TranscodeAudio` everywhere in ErsatzRS:

- Rename the SQLite and MySQL `FFmpegProfile` column from `NormalizeAudio` to
  `TranscodeAudio` in paired incremental migrations, preserving every stored
  boolean value.
- Rename the ErsatzRS Rust domain, persistence, server-function, and admin UI
  fields to `transcode_audio` / `TranscodeAudio`.
- Label the localized admin checkbox "Transcode Audio" and use equivalent
  translations for every supported culture.
- Retain the existing behavior: false forces `AudioFormat = Copy`; true uses
  the selected audio codec/settings. `NormalizeLoudnessMode` remains the only
  loudness-normalization control.

The original collapsed initial migrations remain untouched, so installed
databases migrate forward and their recorded migration checksums remain valid.

## Consequences

ErsatzRS database and payload names will no longer be schema-identical to
ErsatzTV for this one field. The name now accurately communicates the
transcode-versus-copy decision, while profile behavior and stored boolean
values remain unchanged.

Existing ErsatzRS installations upgrade automatically at startup. Older
ErsatzRS binaries cannot read a database after this migration, consistent with
the project's existing forward-only migration and downgrade-health model.

## Alternatives Considered

- Keep the upstream identifier and change only the label. Rejected because the
  misleading backend and database names would continue to confuse operators
  and maintainers.
- Rename the setting to `AllowAudioPassthrough`. Rejected because it inverts
  the existing boolean values and would make migrations and APIs less clear.
- Treat the field as loudness normalization. Rejected because LoudNorm is a
  separate upstream setting and FFmpeg filter.

## References

- `$ERSATZTV_SRC/ErsatzTV/Application/FFmpegProfiles/Commands/UpdateFFmpegProfileHandler.cs`
- `$ERSATZTV_SRC/ErsatzTV/Core/FFmpeg/FFmpegPlaybackSettingsCalculator.cs`
- `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs`
- [ADR 0004: Collapse the initial database schema](0004-collapsed-initial-schema.md)
