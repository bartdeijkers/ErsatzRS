# ADR 0050: Name FFmpeg profile video transcoding explicitly

## Status

Accepted (2026-07-21)

## Implementation

Implemented (2026-07-21) — paired SQLite/MySQL migrations, Rust domain and
persistence fields, server-function forms and queries, localized admin UI, and
focused migration/browser regression coverage now use `TranscodeVideo` while
preserving the established transcode-versus-copy behavior.

## Context

ErsatzTV stores an `FFmpegProfile.NormalizeVideo` boolean and labels its
editor checkbox "Normalize Video". The upstream create/update handlers use the
boolean as a save-time gate: false persists `VideoFormat = Copy` and disables
hardware acceleration; true persists the selected video codec and acceleration
settings. Playback then acts on `VideoFormat`.

Actual video normalization and processing choices are separate profile fields,
including `NormalizeFramerate`, `NormalizeColors`, scaling/padding, tonemapping,
and deinterlacing. The inherited parent name therefore obscures the real
transcode-versus-copy choice in the same way that `NormalizeAudio` did before
[ADR 0049](0049-transcode-audio-terminology.md).

## Decision

Deliberately diverge from ErsatzTV terminology and call this setting
`TranscodeVideo` everywhere in ErsatzRS:

- Rename the SQLite and MySQL `FFmpegProfile` column from `NormalizeVideo` to
  `TranscodeVideo` in paired incremental migrations, preserving every stored
  boolean value.
- Rename the ErsatzRS Rust domain, persistence, server-function, and admin UI
  fields to `transcode_video` / `TranscodeVideo`.
- Label the localized admin checkbox "Transcode Video" and use equivalent
  translations for every supported culture.
- Retain the existing behavior: false forces `VideoFormat = Copy` and
  `HardwareAcceleration = None`; true uses the selected video codec/settings.
  The distinct video-processing controls retain their current names.

HTTP Live Streaming Direct continues to copy video independently of the
profile switch. The original collapsed initial migrations remain untouched so
installed databases migrate forward without invalidating recorded checksums.

## Consequences

ErsatzRS database and payload names will no longer be schema-identical to
ErsatzTV for this field. The replacement accurately communicates the
transcode-versus-copy decision while preserving profile behavior and stored
boolean values.

Existing ErsatzRS installations upgrade automatically at startup. Older
ErsatzRS binaries cannot read a database after this migration, consistent with
the project's forward-only migration and downgrade-health model.

## Alternatives Considered

- Keep the upstream identifier and change only the label. Rejected because the
  misleading backend and database terminology would remain.
- Treat `NormalizeVideo` as an umbrella for framerate and color normalization.
  Rejected because those operations already have independent persisted fields,
  and playback uses this boolean only indirectly through `VideoFormat = Copy`.
- Rename the setting to `AllowVideoPassthrough`. Rejected because it would
  invert the existing boolean values and make migrations and APIs less clear.

## References

- `$ERSATZTV_SRC/ErsatzTV/Application/FFmpegProfiles/Commands/UpdateFFmpegProfileHandler.cs`
- `$ERSATZTV_SRC/ErsatzTV/Core/FFmpeg/FFmpegPlaybackSettingsCalculator.cs`
- [ADR 0049: Name FFmpeg profile audio transcoding explicitly](0049-transcode-audio-terminology.md)
