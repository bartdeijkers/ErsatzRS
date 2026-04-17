# Persistence Gap Matrix

This matrix tracks repository and real-world decode coverage for the 138
ErsatzTV tables represented by `migrations/0001_initial.sql`.

Scope:

- "Read" and "Write" mean explicit SQL access in `crates/ersatzrs-infra`.
- "Real-world decode" means the env-gated
  `ERSATZRS_REAL_WORLD_SQLITE` compatibility test reaches the table directly,
  through a parent repository load, or through the scheduling read model when
  the local database contains matching rows.
- The matrix intentionally does not include private paths, database filenames,
  media titles, or other local database values.

Current summary:

- 53 tables have both read and write coverage.
- 29 tables have read-only coverage.
- 56 tables have no infra read/write coverage yet.

Recommended order:

- `P0`: keep broadening env-gated real-world decode for already-covered
  repositories.
- `P1`: implement highest-risk media metadata/stream and scheduler state/item
  subtype tables that affect playback, search, and playout parity.
- `P2`: complete owner repository writes for existing read models and add small
  repositories for group/lookup tables used by editors.
- `P3`: provider-specific scanner persistence tables.
- `P4`: Trakt/rerun collection tables.
- `P9`: preserve Entity Framework bookkeeping only for imported database
  compatibility.

| Table | Existing Rust domain/repository coverage | Read coverage | Write coverage | Real-world decode coverage | Highest backwards-compatibility risk | Recommended next order |
| --- | --- | --- | --- | --- | --- | --- |
| `Actor` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Metadata search can miss imported cast fields. | P1 media metadata/search joins |
| `Artist` | Domain model plus scheduling container expansion | Partial: scheduling container expansion | No | Partial, env-gated scheduling decode if data exists | Artist containers may not expand consistently for music-video scheduling. | P1 media metadata/search joins |
| `ArtistMetadata` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Artist search/container details are read-only. | P1 media metadata/search joins |
| `Artwork` | Channel repository | Yes: Channel repository | Yes: Channel repository updates channel links | Yes, through channel decode | Channel icons can drift if artwork ownership is expanded beyond channel links. | P2 metadata/artwork owner repository |
| `Block` | Block repository and scheduling read model | Yes: Block repository, scheduling read model | Yes: Block repository | Yes, env-gated direct decode | Covered scalar block shape, but item subtype details are still thin. | P1 block item subtype loading |
| `BlockGroup` | Domain model only | No | No | No | Block editor cannot round-trip imported groups. | P2 group repositories |
| `BlockItem` | Block repository and scheduling read model | Yes: Block repository, scheduling read model | Yes: Block repository writes placeholder item rows | Yes, through block decode | Placeholder writes can lose collection/media target fields. | P1 block item target round-trip |
| `BlockItemGraphicsElement` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Block overlays are not editable/writable yet. | P1 block overlay joins |
| `BlockItemWatermark` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Block watermarks are not editable/writable yet. | P1 block overlay joins |
| `Channel` | Channel repository | Yes: Channel repository | Yes: Channel repository | Yes, env-gated direct decode | Low; continue edge-case decode for enum/time fields. | P0 broaden real-world cases |
| `ChannelWatermark` | Scheduling/deco references only | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Watermark definitions cannot be round-tripped outside references. | P2 watermark repository |
| `Collection` | Collection repository and scheduling read model | Yes: Collection repository, scheduling read model | Yes: Collection repository | Yes, env-gated direct decode | Covered basic collection rows; custom playback order semantics remain thin. | P2 collection editor parity |
| `CollectionEnumeratorState` | Playout repository | Yes: Playout repository via `PlayoutProgramScheduleAnchor` | Yes: Playout repository via `replace_runtime_state` | Yes, through playout decode | Low/medium; covered through parent playout state writes and env-gated decode. | P1 done in playout state slice |
| `CollectionItem` | Collection repository and scheduling read model | Yes: Collection repository, scheduling read model | Yes: Collection repository | Yes, through collection decode | Covered, but ordering semantics are limited by ErsatzTV schema shape. | P2 collection ordering behavior |
| `ConfigElement` | Config repository | Yes: Config repository | Yes: Config repository | Yes, env-gated direct decode | Low. | P0 retain env-gated config decode |
| `Deco` | Deco repository, graphics repository, scheduling read model | Yes: Deco repository, graphics repository, scheduling read model | Yes: Deco repository | Yes, env-gated direct decode | Deco fallback/dead-air fields are partly defaulted on write. | P1 deco full-field round-trip |
| `DecoBreakContent` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Break content placement can diverge from ErsatzTV. | P1 deco break content write/read |
| `DecoGraphicsElement` | Deco and graphics repositories | Yes: Deco/graphics repositories | Yes: Deco repository | Yes, through deco/graphics decode | Low/medium; variables are not represented. | P2 deco overlay edit parity |
| `DecoGroup` | Domain model only | No | No | No | Deco editor cannot round-trip imported groups. | P2 group repositories |
| `DecoTemplate` | Domain model only, scheduling reference tests | No direct repository | No | Partial, env-gated scheduling decode if data exists | Deco template selection is not repository-backed. | P1 deco template repository |
| `DecoTemplateGroup` | Domain model only | No | No | No | Deco template grouping cannot round-trip. | P2 group repositories |
| `DecoTemplateItem` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Deco-template item selection is read-only. | P1 deco template repository |
| `DecoWatermark` | Deco repository | Yes: Deco repository | Yes: Deco repository | Yes, through deco decode | Low/medium. | P2 watermark edit parity |
| `Director` | Domain/core search placeholder only | No | No | No | Director search parity is absent. | P1 media metadata/search joins |
| `EmbyCollection` | Domain model only | No | No | No | Emby collection scanner data cannot round-trip. | P3 provider scanner tables |
| `EmbyConnection` | Media source repository | Yes: Media source repository | Yes: Media source repository | Yes, env-gated direct decode | Low. | P0 broaden provider source decode |
| `EmbyEpisode` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `EmbyLibrary` | Domain model only | No | No | No | Emby library subtype lookup currently defaults rather than verifies row shape. | P3 provider library subtype repository |
| `EmbyMediaSource` | Media source repository | Yes: Media source repository | Yes: Media source repository | Yes, env-gated direct decode | Low. | P0 broaden provider source decode |
| `EmbyMovie` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `EmbyPathInfo` | Schema only | No | No | No | Provider path scanning state is absent. | P3 provider scanner tables |
| `EmbyPathReplacement` | Media source repository | Yes: Media source repository | Yes: Media source repository | Yes, through Emby source decode | Low. | P0 broaden provider source decode |
| `EmbySeason` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `EmbyShow` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `Episode` | Metadata repository join and scheduling read model | Yes: metadata/scheduling read paths | No | Partial, env-gated scheduling decode if data exists | Episode container rows cannot be created by current metadata writes. | P1 media polymorphic repository |
| `EpisodeMetadata` | Metadata repository and scheduling read model | Yes: metadata/scheduling read paths | Yes: Metadata repository | Yes, env-gated direct decode | Covered core fields; normalized joins remain incomplete. | P1 media metadata/search joins |
| `FFmpegProfile` | FFmpeg profile repository | Yes: FFmpeg profile repository | Yes: FFmpeg profile repository | Yes, env-gated direct decode | Low; continue enum compatibility checks. | P0 broaden real-world enum cases |
| `FillGroupEnumeratorState` | Playout repository | Yes: Playout repository via `PlayoutScheduleItemFillGroupIndex` | Yes: Playout repository via `replace_runtime_state` | Yes, through playout decode | Low/medium; covered through parent fill-group state writes and env-gated decode. | P1 done in playout state slice |
| `FillerPreset` | Filler repository | Yes: Filler repository | Yes: Filler repository | Yes, env-gated direct decode | Low/medium; playback-order defaults need parity review. | P2 filler editor parity |
| `Genre` | Scheduling search projection | Partial: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Search/filter metadata is read-only and incomplete across media kinds. | P1 media metadata/search joins |
| `GraphicsElement` | Graphics repository and scheduling read model | Yes: graphics/scheduling read paths | Yes: Graphics repository | Yes, env-gated direct decode | Low/medium; ErsatzRS-only fields are not stored in ErsatzTV schema. | P2 graphics editor parity |
| `Image` | Scheduling read model only | Partial: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Image item scheduling lacks owner repository and metadata decode. | P1 media polymorphic repository |
| `ImageFolderDuration` | Domain model only | No | No | No | Still-image duration parity is absent. | P1 media item detail decode |
| `ImageMetadata` | Domain model only | No | No | No | Image search/display metadata cannot decode. | P1 media metadata/search joins |
| `JellyfinCollection` | Domain model only | No | No | No | Jellyfin collection scanner data cannot round-trip. | P3 provider scanner tables |
| `JellyfinConnection` | Media source repository | Yes: Media source repository | Yes: Media source repository | Yes, env-gated direct decode | Low. | P0 broaden provider source decode |
| `JellyfinEpisode` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `JellyfinLibrary` | Library repository subtype check plus Jellyfin sync bootstrap | Yes: Library repository subtype lookup and Jellyfin sync bootstrap | Yes: Jellyfin sync bootstrap | Yes, through library decode | Library subtype rows can now be created for movie sync; full library update/delete remains thin. | P3 provider library subtype update parity |
| `JellyfinMediaSource` | Media source repository | Yes: Media source repository | Yes: Media source repository | Yes, env-gated direct decode | Low. | P0 broaden provider source decode |
| `JellyfinMovie` | Jellyfin movie sync | Yes: Jellyfin sync lookup by item id | Yes: Jellyfin movie sync | No | Movie provider identity rows now persist during mock-covered sync; real-world decode should be added once sample data exists. | P3 broaden Jellyfin scanner tables |
| `JellyfinPathInfo` | Schema only | No | No | No | Provider path scanning state is absent. | P3 provider scanner tables |
| `JellyfinPathReplacement` | Media source repository | Yes: Media source repository | Yes: Media source repository | Yes, through Jellyfin source decode | Low. | P0 broaden provider source decode |
| `JellyfinSeason` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `JellyfinShow` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `LanguageCode` | Domain model only | No | No | No | Audio/subtitle language parity is absent. | P1 stream/subtitle metadata |
| `Library` | Library repository | Yes: Library repository | Yes: Library repository | Yes, env-gated direct decode | Low/medium; provider subtype writes are incomplete. | P2/P3 library subtype writes |
| `LibraryFolder` | Library repository | Yes: Library repository | No | Yes, through library decode | Folder hierarchy cannot round-trip on write. | P2 library folder writes |
| `LibraryPath` | Library repository | Yes: Library repository | Yes: Library repository | Yes, through library decode | Low/medium; path context for folders is incomplete. | P2 library folder writes |
| `LocalLibrary` | Library repository and local scanner | Yes: Library repository | Yes: Library repository and local scanner preserves subtype rows while updating scan timestamps | Yes, through library decode | Low; local scanner now depends on this subtype for configured library paths. | P0 retain decode |
| `LocalMediaSource` | Media source repository list only | Yes: Media source repository list | No | Yes, through source listing | Local source has no full get/upsert path. | P2 local media source repository |
| `MediaChapter` | ffprobe extraction, local scanner, Jellyfin sync | Yes: scanner/sync tests assert persisted rows | Yes: local scanner and Jellyfin movie sync | No | Chapter rows are now populated for scanned media; owner repository update/delete parity is still narrow. | P3 broaden provider/local scanner coverage |
| `MediaFile` | Media item repository path lookup plus scanner/sync writes | Yes: Media item repository and scanner/sync row lookup | Yes: local scanner and Jellyfin movie sync | Yes, through media item path/decode when present | File rows now persist for local and Jellyfin scans; path-hash compatibility should be compared against ErsatzTV samples. | P3 broaden provider/local scanner coverage |
| `MediaItem` | Media item repository and scheduling read model | Yes: media item/scheduling read paths | Yes: Media item repository | Yes, env-gated direct decode | Covered scalar item state; polymorphic subtypes are incomplete. | P1 media polymorphic repository |
| `MediaSource` | Media source repository | Yes: Media source repository | Yes: Media source repository | Yes, through provider source decode | Low. | P0 retain decode |
| `MediaStream` | ffprobe extraction, local scanner, Jellyfin sync | Yes: scanner/sync tests assert persisted rows | Yes: local scanner and Jellyfin movie sync | No | Stream rows are now populated for scanned media; full color/profile/subtitle fields remain partial. | P3 broaden provider/local scanner coverage |
| `MediaVersion` | Media item repository, scheduling read model, scanner/sync writes | Yes: media item/scheduling read paths plus scanner/sync row lookup | Yes: local scanner and Jellyfin movie sync | Yes, through media item/scheduling decode | Version rows now persist for local and Jellyfin scans; additional media kinds still need owner-specific sync. | P3 broaden provider/local scanner coverage |
| `MetadataGuid` | Scheduling search/GUID lookup | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | GUID resolution is read-only and not complete across owner repos. | P1 media metadata/search joins |
| `Mood` | Domain model only | No | No | No | Music metadata/search facet parity is absent. | P1 media metadata/search joins |
| `Movie` | Scheduling read model plus Jellyfin sync | Yes: scheduling read model and Jellyfin sync lookup | Yes: Jellyfin movie sync | Partial, env-gated scheduling decode if data exists | Movie container rows can now be created by Jellyfin sync; non-provider local movie repository parity remains incomplete. | P3 broaden provider scanner coverage |
| `MovieMetadata` | Metadata repository and scheduling read model | Yes: metadata/scheduling read paths | Yes: Metadata repository | Yes, env-gated direct decode | Covered core fields; normalized joins remain incomplete. | P1 media metadata/search joins |
| `MultiCollection` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Multi-collection editor persistence is absent. | P2 collection editor parity |
| `MultiCollectionItem` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Multi-collection contents are read-only. | P2 collection editor parity |
| `MultiCollectionSmartItem` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Multi-collection smart contents are read-only. | P2 collection editor parity |
| `MusicVideo` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Music-video rows cannot be created by current metadata writes. | P1 media polymorphic repository |
| `MusicVideoArtist` | Domain model only | No | No | No | Music-video artist joins are not decoded. | P1 media metadata/search joins |
| `MusicVideoMetadata` | Metadata repository and scheduling read model | Yes: metadata/scheduling read paths | No | Yes, env-gated direct decode | No write path; artist joins are empty. | P1 media metadata/search joins |
| `OtherVideo` | Scheduling read model plus local scanner | Yes: scheduling read model and local scanner write test | Yes: local scanner | Partial, env-gated scheduling decode if data exists | Other-video rows can now be created by local scans; metadata owner fields remain absent. | P3 broaden local scanner coverage |
| `OtherVideoMetadata` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Other-video metadata has no owner repository. | P1 media metadata/search joins |
| `Playlist` | Collection repository and scheduling read model | Yes: collection/scheduling read paths | No | Yes, env-gated direct decode | Playlist editor writes are absent. | P2 playlist repository writes |
| `PlaylistGroup` | Collection repository and scheduling read model | Yes: collection/scheduling read paths | No | Yes, through playlist decode | Playlist groups are read-only. | P2 playlist repository writes |
| `PlaylistItem` | Collection repository and scheduling read model | Yes: collection/scheduling read paths | No | Yes, through playlist decode | Playlist item order/count/play-all cannot round-trip. | P2 playlist repository writes |
| `Playout` | Playout repository plus channel/graphics/scheduling references | Yes: playout/channel/graphics/scheduling read paths | No | Yes, env-gated direct decode | Playout root rows are not writable yet; all child state, item, and history writes are now covered by `apply_build_result`. | P2 playout root create/update (child writes complete) |
| `PlayoutAnchor` | Playout repository | Yes: Playout repository | Yes: Playout repository via `replace_runtime_state` | Yes, through playout decode | Low/medium; anchor round-trip now preserves schedule resume state. | P1 done in playout state slice |
| `PlayoutBuildStatus` | Playout repository | Yes: Playout repository | Yes: Playout repository | Yes, through playout decode | Low. | P0 retain decode |
| `PlayoutGap` | Playout repository | Yes: Playout repository | Yes: Playout repository via `replace_runtime_state` and `apply_build_result` | Yes, through playout decode | Low; gap writes now covered by both runtime-state replace and full build result apply. | P0 retain coverage |
| `PlayoutHistory` | Playout repository and scheduling read model | Yes: playout/scheduling read paths | Yes: Playout repository via `replace_runtime_state` and `apply_build_result` | Yes, through playout decode | Low/medium; history rows now written through both runtime-state replace and full build result apply; field-subset coverage is complete. | P2 residual field-usage follow-up |
| `PlayoutItem` | Playout, graphics, scheduling repositories | Yes: playout/graphics/scheduling read paths | Yes: Playout repository via `upsert_item` and `apply_build_result` | Yes, env-gated range decode | Low/medium; item, gap, and history writes now covered by `apply_build_result`; only root `Playout` row write missing. | P2 playout root write (item pipeline complete) |
| `PlayoutItemGraphicsElement` | Playout, graphics, scheduling repositories | Yes: playout/graphics/scheduling read paths | Yes: Playout repository | Yes, through playout item decode | Low/medium. | P1 playout overlay parity |
| `PlayoutItemWatermark` | Playout and scheduling repositories | Yes: playout/scheduling read paths | Yes: Playout repository | Yes, through playout item decode | Low/medium. | P1 playout overlay parity |
| `PlayoutProgramScheduleAnchor` | Playout repository | Yes: Playout repository | Yes: Playout repository via `replace_runtime_state` | Yes, through playout decode | Low/medium; schedule-anchor resume rows now round-trip. | P1 done in playout state slice |
| `PlayoutScheduleItemFillGroupIndex` | Playout repository | Yes: Playout repository | Yes: Playout repository via `replace_runtime_state` | Yes, through playout decode | Low/medium; fill-group index rows now round-trip. | P1 done in playout state slice |
| `PlayoutTemplate` | Playout and scheduling repositories | Yes: playout/scheduling read paths | Yes: Playout repository via `replace_runtime_state` | Yes, through playout decode | Low/medium; template/deco-template selection now round-trips with date-window fields intact. | P1 done in playout state slice |
| `PlexCollection` | Domain model only | No | No | No | Plex collection scanner data cannot round-trip. | P3 provider scanner tables |
| `PlexConnection` | Media source repository | Yes: Media source repository | Yes: Media source repository | Yes, env-gated direct decode | Low. | P0 broaden provider source decode |
| `PlexEpisode` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `PlexLibrary` | Library repository subtype check | Yes: Library repository subtype lookup | No | Yes, through library decode | Library subtype rows are read-only. | P3 provider library subtype writes |
| `PlexMediaFile` | Domain model only | No | No | No | Plex media-file scanner data cannot round-trip. | P3 provider scanner tables |
| `PlexMediaSource` | Media source repository | Yes: Media source repository | Yes: Media source repository | Yes, env-gated direct decode | Low. | P0 broaden provider source decode |
| `PlexMovie` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `PlexOtherVideo` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `PlexPathReplacement` | Media source repository | Yes: Media source repository | Yes: Media source repository | Yes, through Plex source decode | Low. | P0 broaden provider source decode |
| `PlexSeason` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `PlexShow` | Domain model only | No | No | No | Provider GUID/type rows cannot round-trip. | P3 provider scanner tables |
| `ProgramSchedule` | Program schedule repository | Yes: Program schedule repository | Yes: Program schedule repository | Yes, env-gated direct decode | Covered root fields; child subtypes are missing. | P1 schedule item subtype repository |
| `ProgramScheduleAlternate` | Program schedule repository | Yes: Program schedule repository | No | Yes, through program schedule decode | Alternate schedules are read-only. | P1 schedule item subtype repository |
| `ProgramScheduleDurationItem` | Schema only | No | No | No | Duration schedule item behavior cannot round-trip. | P1 schedule item subtype repository |
| `ProgramScheduleFloodItem` | Schema only | No | No | No | Flood schedule item behavior cannot round-trip. | P1 schedule item subtype repository |
| `ProgramScheduleItem` | Program schedule repository | Yes: Program schedule repository | Yes: Program schedule repository writes placeholder item rows | Yes, through program schedule decode | Placeholder writes can lose concrete item type and target fields. | P1 schedule item subtype repository |
| `ProgramScheduleItemGraphicsElement` | Domain model only | No | No | No | Schedule item overlays are absent. | P1 schedule overlay joins |
| `ProgramScheduleItemWatermark` | Domain model only | No | No | No | Schedule item watermarks are absent. | P1 schedule overlay joins |
| `ProgramScheduleMultipleItem` | Schema only | No | No | No | Multiple-item schedule behavior cannot round-trip. | P1 schedule item subtype repository |
| `ProgramScheduleOneItem` | Schema only | No | No | No | One-item schedule behavior cannot round-trip. | P1 schedule item subtype repository |
| `RemoteStream` | Scheduling read model | Partial: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Remote stream media rows lack owner repository. | P1 media polymorphic repository |
| `RemoteStreamMetadata` | Domain model only | No | No | Partial, env-gated scheduling decode if data exists | Remote stream metadata decode is incomplete. | P1 media metadata/search joins |
| `RerunCollection` | Domain model only | No | No | No | Rerun collection feature is not persisted. | P4 rerun collections |
| `RerunHistory` | Domain model only | No | No | No | Rerun history parity is absent. | P4 rerun collections |
| `Resolution` | Domain model only | No | No | No | Stream/dimension search and transcode decisions cannot use imported rows. | P1 media stream repository |
| `ScheduleItemsEnumeratorState` | Playout repository | Yes: Playout repository | Yes: Playout repository via `replace_runtime_state` | Yes, through playout anchor decode | Low/medium; anchor enumerator state now round-trips. | P1 done in playout state slice |
| `Season` | Metadata repository join and scheduling read model | Yes: metadata/scheduling read paths | No | Partial, env-gated scheduling decode if data exists | Season rows cannot be created by owner repository. | P1 media polymorphic repository |
| `SeasonMetadata` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Season metadata has no owner repository. | P1 media metadata/search joins |
| `Show` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Show rows cannot be created by owner repository. | P1 media polymorphic repository |
| `ShowMetadata` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Show metadata has no owner repository. | P1 media metadata/search joins |
| `SmartCollection` | Collection repository and scheduling read model | Yes: collection/scheduling read paths | No | Yes, env-gated direct decode | Smart collection editor writes are absent. | P2 collection editor parity |
| `Song` | Scheduling read model | Partial: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Song rows cannot round-trip. | P1 media polymorphic repository |
| `SongMetadata` | Scheduling read model | Yes: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Song metadata has no owner repository. | P1 media metadata/search joins |
| `Studio` | Scheduling search projection | Partial: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Studio search/filter metadata is read-only. | P1 media metadata/search joins |
| `Style` | Domain model only | No | No | No | Music metadata/search facet parity is absent. | P1 media metadata/search joins |
| `Subtitle` | Domain model only | No | No | No | Subtitle selection/search fields are absent. | P1 stream/subtitle metadata |
| `Tag` | Scheduling search projection | Partial: Scheduling read model | No | Partial, env-gated scheduling decode if data exists | Tag search/filter metadata is read-only. | P1 media metadata/search joins |
| `Template` | Template repository and scheduling read model | Yes: template/scheduling read paths | Yes: Template repository | Yes, env-gated direct decode | Root covered; child item writes remain missing. | P1 template item write parity |
| `TemplateGroup` | Domain model only | No | No | No | Template editor cannot round-trip groups. | P2 group repositories |
| `TemplateItem` | Template repository and scheduling read model | Yes: template/scheduling read paths | No | Yes, through template decode | Template item writes are absent. | P1 template item write parity |
| `TraktList` | Domain model only | No | No | No | Trakt list feature is not persisted. | P4 Trakt collections |
| `TraktListItem` | Domain model only | No | No | No | Trakt list item feature is not persisted. | P4 Trakt collections |
| `TraktListItemGuid` | Domain model only | No | No | No | Trakt GUID matching is not persisted. | P4 Trakt collections |
| `Writer` | Domain/core search placeholder only | No | No | No | Writer search parity is absent. | P1 media metadata/search joins |
| `__EFMigrationsHistory` | Schema compatibility only | No | No | No | Low; required only so imported EF databases keep expected bookkeeping shape. | P9 preserve only |
| `__EFMigrationsLock` | Schema compatibility only | No | No | No | Low; required only so imported EF databases keep expected bookkeeping shape. | P9 preserve only |
