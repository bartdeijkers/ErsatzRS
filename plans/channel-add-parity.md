# Plan: Port `/channels/add` Channel Editor Parity

## Context

The user request names `/channel/add`; the active ErsatzTV and ErsatzRS route is
`/channels/add` (plural), implemented by `ErsatzTV/Pages/ChannelEditor.razor`
and `crates/ersatzrs-web/src/admin/pages/channels/editor.rs`.

The reference screenshots in `plans/references/add_channel_ErsatzTV.png` and
`plans/references/add_channel_ErsatzRS_2026-05-21.png` show the current gap:
ErsatzTV exposes the complete channel editor contract, while ErsatzRS currently
edits only number, name, group, streaming mode, FFmpeg profile, fallback filler,
enabled, and EPG visibility. This plan targets full functional parity for both
create and edit because both routes share the same editor and save path.

## Authoritative sources

- ErsatzTV page: `$ERSATZTV_SRC/ErsatzTV/Pages/ChannelEditor.razor`
- ErsatzTV view model: `$ERSATZTV_SRC/ErsatzTV/ViewModels/ChannelEditViewModel.cs`
- ErsatzTV validation: `$ERSATZTV_SRC/ErsatzTV/Validators/ChannelEditViewModelValidator.cs`
- ErsatzTV commands:
  - `$ERSATZTV_SRC/ErsatzTV.Application/Channels/Commands/CreateChannel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Channels/Commands/CreateChannelHandler.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Channels/Commands/UpdateChannel.cs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Channels/Commands/UpdateChannelHandler.cs`
- ErsatzTV projection: `$ERSATZTV_SRC/ErsatzTV.Application/Channels/ChannelViewModel.cs`
  and `$ERSATZTV_SRC/ErsatzTV.Application/Channels/Mapper.cs`
- ErsatzRS current page and save path:
  - `crates/ersatzrs-web/src/admin/pages/channels/editor.rs`
  - `crates/ersatzrs-web/src/admin/server_fns/channels.rs`
  - `crates/ersatzrs-web/src/api/channels.rs`
  - `crates/ersatzrs-infra/src/repositories/channel.rs`
- Schema contract: `migrations/0001_initial.sql` and
  `migrations/mysql/0001_initial.sql` (`Channel`, `Artwork`,
  `ChannelWatermark`, `FillerPreset`, `FFmpegProfile`)

## Required parity

### Editor fields

The ErsatzRS editor must expose and persist every editable field from
`ChannelEditViewModel`:

- Channel basics: `Number`, `Name`, `Group`, `Categories`
- Runtime behavior: `TranscodeMode` (disabled, `On Demand`), `IdleBehavior`
- Playout source: `PlayoutSource`, generated `PlayoutMode`, mirror
  `MirrorSourceChannelId`, mirror `PlayoutOffset` in hours
- Flags: `IsEnabled`, `ShowInEpg`
- Streaming: `StreamingMode`, `FFmpegProfileId`, `SlugSeconds`
- Stream selection: `StreamSelectorMode`, default-mode preferred audio
  language/title, preferred subtitle language, `SubtitleMode`, and custom-mode
  `StreamSelector`
- Music-video behavior: `MusicVideoCreditsMode`,
  `MusicVideoCreditsTemplate`, `SongVideoMode`
- Channel logo: uploaded logo, `ExternalLogoUrl`, logo preview, delete/replace
  semantics through empty logo data
- Watermark: `WatermarkId`
- Fallback filler: `FallbackFillerId`, filtered to fallback presets

### Data sources

The editor resource must load the same supporting lists that ErsatzTV loads:

- all channels, including playout count, so generated channels can be selected
  as mirror sources and playout-source changes can be disabled when a generated
  channel already has a playout
- all FFmpeg profiles
- all language codes for audio/subtitle selects
- all channel watermarks
- fallback filler presets only
- music-video credit template names
- channel stream selector names
- FFmpeg settings default profile for new-channel defaults

### Defaults

New channel defaults must match ErsatzTV:

- number = next whole-number channel after the current maximum first segment
- name = `New Channel`
- group = `ErsatzTV`
- `FFmpegProfileId` = configured default FFmpeg profile
- `StreamingMode` = `TransportStreamHybrid` (`5`, label `MPEG-TS`)
- `IsEnabled` = true
- `ShowInEpg` = true
- all enum defaults as defined by the ErsatzTV domain enums

### Validation and save behavior

Server-side save must enforce the ErsatzTV command validators, not just UI
checks:

- number matches `Channel.NumberValidator` / `Channel::NUMBER_PATTERN`
- number is unique on create and unique except self on update
- name is non-empty and at most 50 characters
- group is non-empty
- FFmpeg profile exists
- watermark exists when selected
- fallback filler exists and is `FillerKind.Fallback` when selected
- external logo URL is valid by ErsatzTV's `Artwork.IsExternalUrl` behavior
- disabled channels cannot be shown in EPG; persisted `ShowInEpg` is
  `IsEnabled && ShowInEpg`
- mirror source is required for mirror channels and must point to a generated
  channel other than self
- mirror playout offset is between -12 and +12 hours when supplied
- mirror channels persist `PlayoutMode=Continuous`; generated channels clear
  mirror source and offset
- updates must preserve ErsatzTV's "do not save mirror when playout exists"
  behavior
- update side effects must enqueue subtitle extraction when subtitle mode is not
  `None` and refresh channel/guide data when the equivalent Rust background
  plumbing exists; missing infrastructure must be added in the real slice
  rather than bypassed silently

### Logo behavior

Logo handling must match ErsatzTV's command semantics:

- uploaded logos are stored under the IPTV logo cache shape and persisted as
  `ArtworkKind.Logo`
- `iptv/logos/{file}` form paths are normalized to the DB `Artwork.Path`
  filename
- external logo URL stores the external URL in `Artwork.Path` with empty content
  type
- update replaces an existing logo row, creates a logo row when absent, and
  removes logo artwork when no logo/external URL is supplied
- rendered channel playlists and list logos must prefer stored channel logo
  artwork when available and fall back to generated text logo otherwise

### UI and hydration constraints

- Render the editor as a fully functional SSR form until the broader Leptos
  island marker mismatch is fixed. This is the parity path, not a stopgap: every
  channel editor field must be visible, editable, posted, validated, and
  persisted without client hydration.
- Keep any future hydrated editor stable. Conditional generated/mirror and
  default/custom selector sections must use stable rendered containers with
  `style:display`/`disabled` instead of `<Show>` inside an island.
- Match ErsatzTV's field ordering and labels from `ChannelEditor.razor`.
- Use compact admin-form styling appropriate for repeated operational use;
  avoid card nesting.
- The FFmpeg profile and watermark selects are disabled when streaming mode is
  `HttpLiveStreamingDirect`, matching ErsatzTV.
- `MusicVideoCreditsTemplate` is disabled unless
  `MusicVideoCreditsMode=GenerateSubtitles`.
- `PlayoutSource` is disabled when editing a generated channel that already has
  at least one playout.

## Ordered implementation

### 1. Core save contract

Add a typed channel-editor save/read contract in `ersatzrs-core` so validation
and persistence do not remain ad hoc SQL in the web crate:

- extend or add repository methods for channel editor projection, supporting
  list projections, and create/update
- define typed result/error structs for validation errors that can be returned
  to the Leptos form and the HTML form fallback
- reuse existing domain enums and `Channel::NUMBER_PATTERN`

### 2. Infra persistence parity

Implement the channel editor repository in `ersatzrs-infra`:

- read full channel editor projection including logo artwork and playout count
- read all supporting lists with SQLite/MySQL-compatible SQL
- validate and persist create/update in one transaction
- write/delete `Artwork` rows for channel logos with `ArtworkKind=2`
- preserve all existing `Channel` fields when editing
- update `ChannelRepository::upsert` only if needed for shared behavior; do not
  route editor saves through raw web SQL once the repository slice exists

### 3. Web server functions and API fallback

Expand `crates/ersatzrs-web/src/admin/server_fns/channels.rs`:

- replace `ChannelForm` with the full editor form DTO
- return one editor resource DTO containing form data plus profiles, languages,
  watermarks, fallback fillers, mirror candidates, template names, selectors,
  and relevant edit locks
- expose `save_channel` through the same typed validation path
- keep `/api/channels/save` as a functional no-JS fallback and expand its form
  parser to the full contract, including multipart logo upload if the final
  HTML form uses file input

### 4. Logo upload and serving

Add or extend plain axum endpoints for channel logo upload:

- enforce the same maximum upload size path used by watermark upload
- persist uploaded files into a channel-logo cache path compatible with
  `iptv/logos/{file}` form values
- serve stored logos under `/iptv/logos/{file}` and keep `/iptv/logos/gen`
  fallback
- update IPTV playlist/logo projection to use stored logo artwork before the
  generated text logo

### 5. Editor UI

Rebuild `channels/editor.rs` around the full form:

- keep route handling for `/channels/add` and `/channels/:id`
- render fields in ErsatzTV order
- expose generated/mirror and default/custom selector fields in the SSR form so
  dependent values remain editable without client handlers
- upload logo through the multipart SSR save path and show the stored/external
  preview after save/reload
- show server validation errors at form level
- preserve the no-JS form post path

### 6. Background side effects

Port the concrete side effects reachable from channel create/update:

- refresh channel list after create/update
- refresh channel guide/data when playout source or EPG visibility changes
- request embedded subtitle extraction for the channel's playout when subtitle
  mode becomes non-`None`
- mark any not-yet-existing worker messages as Phase 7 implementation work and
  add them in the same slice before calling parity complete

### 7. Tests and parity docs

Add tests at the narrowest useful levels:

- infra create/update tests covering all `Channel` columns, logo artwork
  create/update/delete, mirror validation, uniqueness, and fallback filler kind
- web/server-fn tests for defaults, supporting lists, validation error mapping,
  and no-JS form parsing
- hydration lint coverage with no new findings
- update `docs/reference/parity-matrix.md`, `TEST.md` per-page notes if the
  page status changes, and `STATUS.md` session log

## Verification

Completion requires evidence for every item above:

1. `cargo fmt --all -- --check`
2. `cargo xtask hydration-lint -p ersatzrs-web`
3. `cargo clippy -p ersatzrs-core --all-targets -- -D warnings`
4. `cargo clippy -p ersatzrs-infra --all-targets -- -D warnings`
5. `cargo clippy -p ersatzrs-web --all-targets -- -D warnings`
6. `cargo test -p ersatzrs-infra --all-features channel`
7. `cargo test -p ersatzrs-web --all-features channel`
8. `(cd crates/ersatzrs-web && cargo leptos build)`
9. Manual browser smoke against `/channels/add` and `/channels/:id`:
   default values, all select options, generated/mirror fields, default/custom
   selector fields, logo upload/external URL/preview, disabled channel EPG
   handling, save/reload round-trip, stored logo in playlist/list output.

## Completion criteria

`/channels/add` is complete only when the visible UI, saved database state,
logo behavior, validation failures, side effects, and edit round-tripping match
the ErsatzTV sources listed above. A field that is merely displayed but not
persisted, a persisted value without validation parity, or a UI-only fallback
without repository-backed behavior does not count as parity.
