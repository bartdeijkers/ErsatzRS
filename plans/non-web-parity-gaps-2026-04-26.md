# Non-web parity gaps tussen ErsatzTV en ErsatzRS

## Slice status overview

Last reviewed: 2026-05-01.

| Status | Slices | Notes |
| --- | --- | --- |
| Completed | Slice 1 — HLS entry surface | Landed before this non-web follow-up plan; included here for full Phase 7 slice context. |
| Completed | Slice 2 — HLS segmenter worker | Landed before this non-web follow-up plan; GO blockers closed. |
| Completed | Slice 3 — HLS playlist management | Landed before this non-web follow-up plan. |
| Completed | Slice 4 — HLS-direct passthrough | Landed before this non-web follow-up plan. |
| Completed | Slice 5 — IPTV JWT enforcement | Landed before this non-web follow-up plan. |
| Completed | Slice 6 — Remaining hardware builders | Landed before this non-web follow-up plan; granular decoder dispatch remains deferred to Slice 11. |
| Completed | Slice 7 — MySQL backend support | Landed before this non-web follow-up plan. |
| Completed | Slice 8 — Trakt list integration | Landed before this non-web follow-up plan. |
| Completed | Slice 9 — Readiness + downgrade/schema checks | Landed before this non-web follow-up plan. |
| Completed | Slice 10 — FFmpeg 8.1 runtime report + Docker delivery | Landed before this non-web follow-up plan; Slice 12 builds on the runtime report. |
| Completed | Slice 11 — `PipelineRequest` source-codec metadata + decoder dispatch | Source video/audio metadata, known decoder table, hardware/source-codec dispatch, and argv snapshot updates are complete. |
| Completed | Slice 12 — Capability detection + `AvailableEncoders/Decoders` | Runtime-report-backed hardware capability checks now gate encoder/decoder selection and fall back to software with snapshot coverage. |
| Partial | Slice 16 — Health check engine | Core health contracts, SQL-backed checks, and `ConfigElement` persistence are in place; automatic run wiring, web consumption of `health.summary`, and deeper typed hardware probes remain open. |
| Open | Slice 13 — Ontbrekende FFmpeg-filters | Not started in this plan. |
| Open | Slice 14 — `ProgramScheduleItem` subtype repositories | Not started in this plan. |
| Open | Slice 15 — Classic scheduling engine | Not started in this plan. |
| Open | Slice 17 — Background maintenance loops | Not started in this plan. |
| Open | Slice 18 — Scanner deepening | Not started in this plan. |
| Open | Slice 19 — Search index hardening | Not started in this plan. |
| Open | Slice 20 — Config + ADR-afronding | Not started in this plan. |
| Open | Slice 21 — Phase 7 closure: benchmarks + soak | Not started in this plan; closure slice depends on the earlier slices. |

## Context

ErsatzRS is in Phase 7 (Hardening and Parity Completion). De master plan-items
voor Phase 7 die al gesloten zijn (Slices 1–10) bestrijken HLS, extra hardware
builders, MySQL, Trakt, readiness/schema-checks en Docker-imagery. STATUS.md
beschrijft de huidige staat als "ready for review" voor de meest recente
slices, maar er zijn nog substantiële parity-gaten tussen ErsatzTV en ErsatzRS
buiten de admin UI om. Dit plan inventariseert die gaten en zet ze in
uitvoeringsvolgorde.

**Scope-keuzes (door gebruiker bevestigd):**

- De volledige `crates/ersatzrs-web/` is buiten scope — admin UI én
  `src/transport`/`src/api`/`src/host` modules. Dit plan raakt alleen
  `ersatzrs-ffmpeg`, `ersatzrs-core`, `ersatzrs-infra`, `ersatzrs-scanner`,
  `ersatzrs-domain`, en `migrations/`.
- Classic scheduling engine wordt **wél** geport (One/Multiple/Duration/Flood)
  inclusief de `ProgramScheduleItem`-subtype repositories.
- FFmpeg pipeline-parity wordt **volledig** ingehaald: decoder dispatch,
  capability detection, en de ontbrekende filters.

## Geïnventariseerde gaten

### A. FFmpeg pipeline (`crates/ersatzrs-ffmpeg/`) — LARGE

Huidige Rust crate is plat (`pipeline/mod.rs` één file, `filter/{simple,complex}.rs`,
één builder per hwaccel zonder factory) tegenover ErsatzTV's gelaagde
`Pipeline/`, `Decoder/`, `Encoder/`, `Filter/` en `Capabilities/`-mappen.

| Subgap | C# referentie | Rust status |
| --- | --- | --- |
| Decoder dispatch tables (per-codec hwaccel) | `Decoder/Cuvid/`, `Decoder/Qsv/`, `Decoder/V4l2m2m/`, `DecoderH264/Hevc/Av1/Mpeg{1,2,4}/Vp9/Vc1.cs` | Ontbreekt — `PipelineRequest` draagt geen source-codec metadata |
| Capability factory + caching | `Capabilities/HardwareCapabilitiesFactory.cs`, `FFmpegKnownDecoder/Encoder/Filter/Format/HardwareAcceleration.cs` | Slice 10 introduceerde startup-runtime report; geen typed `IHardwareCapabilities`-traits met fallback |
| `AvailableEncoders` factory + fallback | `Encoder/AvailableEncoders.cs`, `Encoder/Nvenc/Qsv/Amf/Vaapi/Rkmpp/V4l2m2m/VideoToolbox/` | Hardcoded match in `pipeline::video_encoder()` zonder capability-gating |
| Filter: deinterlace per hwaccel | `Filter/YadifFilter.cs` + `Filter/Cuda/YadifCudaFilter.cs` + `Filter/Qsv/DeinterlaceQsvFilter.cs` + `Filter/Vaapi/DeinterlaceVaapiFilter.cs` | Alleen software yadif |
| Filter: tonemap | `Filter/TonemapFilter.cs` (+ Cuda/Qsv/Vaapi varianten) | Ontbreekt |
| Filter: normalize loudness | `Filter/NormalizeLoudnessFilter.cs` (loudnorm + aresample two-pass) | Ontbreekt |
| Filter: colorspace migratie | `Filter/ColorspaceFilter.cs` (zscale + colorspace, bt.2020→bt.709) | Ontbreekt |
| Filter: hardware up/download per accel | `Filter/HardwareUploadFilter.cs`, `HardwareDownloadFilter.cs`, `Cuda/CudaFormatFilter.cs`, `Qsv/QsvFormatFilter.cs`, `Vaapi/VaapiFormatFilter.cs` | Inline hwdownload binnen scaling, geen expliciete upload/download nodes |
| Filter: framerate `fps`+`setpts` chain | `Filter/FrameRateFilter.cs` | `reset_pts_filter` aanwezig, geen fps-precisie/rounding |
| `PipelineContext` flags (`is_hdr`, `should_deinterlace`) | `Pipeline/PipelineContext.cs` | `FrameState` mist deze velden |
| Concat metadata (service name/provider, infinite loop, readrate) | `PipelineBuilderBase.Concat()` | `concat.rs` schrijft alleen ffconcat-file |

### B. Classic scheduling engine + schedule-item subtype writes — LARGE

| Subgap | C# referentie | Rust status |
| --- | --- | --- |
| Classic engine (One/Multiple/Duration/Flood) | `Core/Scheduling/PlayoutModeScheduler{One,Multiple,Duration,Flood}.cs` + `PlayoutModeSchedulerBase.cs` | `PlayoutBuilder` retourneert `UnsupportedScheduleKind` voor Classic |
| `ProgramScheduleItem` subtype rijen | `Core/Domain/ProgramSchedule{One,Multiple,Duration,Flood}Item.cs` + EF mapping | Tabellen bestaan in `0001_initial.sql`; in `program_schedule.rs` worden alleen placeholder-rijen geschreven (zie `persistence-gap-matrix.md` — alle vier gemarkeerd "Schema only") |
| `ProgramScheduleAlternate` write | repository `program_schedule.rs` | Read-only |
| `ProgramScheduleItemWatermark` / `ProgramScheduleItemGraphicsElement` | Domain only | Geen read/write |
| `BlockItem` typed target write voor nieuwe IDs | `block.rs` | Bestaande rijen worden bewaard; nieuwe IDs schrijven nu placeholder; collection/show/season/searchquery target-velden ontbreken |
| `BlockItemWatermark` / `BlockItemGraphicsElement` write | scheduling read model | Read-only (zie persistence-gap-matrix) |
| `FixedStartTimeBehavior` (Flexible/Fixed/FlexibleOverflow) | `Core/Scheduling/FixedStartTimeBehavior.cs` + `PlayoutModeSchedulerBase.GetStartTimeAfter` | Niet geport |
| `OnDemandCheckpoint` handler | `Application/Playouts/Commands/UpdateOnDemandCheckpointHandler.cs` | Veld bestaat in `Playout`; geen update-pad |

### C. Persistence parity P1 (`crates/ersatzrs-infra/src/repositories/`) — MEDIUM

Uit `docs/reference/persistence-gap-matrix.md` (alleen P1-rijen, ~24 tabellen):

- **Schedule item subtypes** — `ProgramScheduleDurationItem`, `ProgramScheduleFloodItem`,
  `ProgramScheduleMultipleItem`, `ProgramScheduleOneItem` (alle "Schema only").
- **Block item targets + overlays** — `BlockItemWatermark`, `BlockItemGraphicsElement`,
  block item typed target round-trip.
- **Schedule item overlays** — `ProgramScheduleItemWatermark`, `ProgramScheduleItemGraphicsElement`.
- **Media metadata/search joins** — `Actor`, `Genre`, `Tag`, `Studio`, `Mood`,
  `Style`, `Director`, `Writer`, `MetadataGuid`, `MusicVideoArtist`,
  `MusicVideoMetadata` (write), `OtherVideoMetadata`, `SeasonMetadata`,
  `ShowMetadata`, `SongMetadata`, `ArtistMetadata`.
- **Polymorphic media owners** — `Image`/`ImageMetadata`/`ImageFolderDuration`,
  `MusicVideo`, `RemoteStream`/`RemoteStreamMetadata`, `Song` owner repos.
- **Stream/subtitle metadata** — `Subtitle`, `LanguageCode`, `Resolution`.
- **Deco templates + break content** — `DecoTemplate`, `DecoTemplateItem` (read-only),
  `DecoBreakContent` (read-only), `Deco` full-field round-trip.
- **Templates** — `Template` root covered, `TemplateItem` writes ontbreken.
- **Playout overlays** — `PlayoutItemGraphicsElement`/`PlayoutItemWatermark` field-usage follow-up.

### D. Health check engine — MEDIUM — PARTIAL

Slice 16 is gestart. `crates/ersatzrs-core/src/health.rs` bevat nu de
ErsatzTV-compatibele `HealthCheckStatus`, `HealthCheckResult`,
`HealthCheckSummary`, `HealthCheck` trait en `HealthCheckService` runner.
`crates/ersatzrs-infra/src/health.rs` bevat de eerste SQL-backed health checks
en kan de laatste run in `ConfigElement` persistenteren onder `health.results`
en `health.summary`.

Wat nog ontbreekt: de web/admin badge leest nog de oude startup/runtime telling
en niet de gepersisteerde `health.summary`; er is nog geen background/run-once
hook die de engine automatisch draait; provider-connectivity checks zijn
voorlopig state-based in plaats van echte Plex/Jellyfin/Emby online checks; en
de hardware checks gebruiken de Slice-10 capability report in plaats van de
volledige ErsatzTV `IHardwareCapabilities`-factory met per-profiel probe.

| Check | C# referentie | Rust |
| --- | --- | --- |
| FFmpegVersion | `Infrastructure/Health/Checks/FFmpegVersionHealthCheck.cs` | Geport tegen `FfmpegRuntimeReport`; startup blijft fail-fast |
| FFmpegCapabilities | `FFmpegCapabilitiesHealthCheck.cs` | Geport tegen Slice-10 capability issues |
| HardwareAcceleration | `HardwareAccelerationHealthCheck.cs` | Eerste check geport: detecteert ondersteunde accel uit `hwaccels`/encoders en waarschuwt over kanaalprofielen zonder ondersteunde accel |
| VaapiDriver | `VaapiDriverHealthCheck.cs` | Eerste check geport: actieve VAAPI-profielen, ontbrekende VAAPI capability en default-driver info; mist nog per-device/driver capability probe |
| FileNotFound | `FileNotFoundHealthCheck.cs` | Geport tegen `MediaItem.State = FileNotFound` met voorbeeldpaden |
| EmptySchedule | `EmptyScheduleHealthCheck.cs` | Geport voor schedules met playout/alternate maar zonder items |
| ZeroDuration | `ZeroDurationHealthCheck.cs` | Geport tegen `MediaVersion.Duration = 00:00:00` met voorbeeldpaden |
| MovieMetadata / EpisodeMetadata | `MovieMetadataHealthCheck.cs`, `EpisodeMetadataHealthCheck.cs` | Geport voor ontbrekende metadata-rijen met voorbeeldfolders |
| Unavailable (provider offline) | `UnavailableHealthCheck.cs` | Eerste check geport tegen `MediaItem.State = Unavailable`; echte provider-offline/connectivity polling blijft open |
| Downgrade | `DowngradeHealthCheck.cs` | Geport tegen `DatabaseSchemaStatus` unknown/dirty/mismatched migration state |

### E. Background maintenance — MEDIUM

`crates/ersatzrs-web/src/host/tasks.rs` start nu alleen UDP-discovery en een
hourly Trakt loop. ErsatzTV's `Services/SchedulerService.cs`, `WorkerService.cs`,
`ScannerService.cs`, `SearchIndexService.cs`, en `FFmpegWorkerService.cs`
hebben geen Rust-tegenhanger. Omdat de spawn-locatie in `host/tasks.rs` ligt
(buiten scope), komen de **runtime-onafhankelijke worker-loops** in
`ersatzrs-core` en `ersatzrs-infra`; de bin-crate (`ersatzrs-scheduler`, nieuw)
draait ze los.

| Loop | C# | Aanbevolen Rust-locatie |
| --- | --- | --- |
| Periodic playout reset/build (30 min) | `SchedulerService.cs` | nieuwe `ersatzrs-core/src/maintenance.rs` + `ersatzrs-scheduler` bin |
| Library refresh dispatcher (`Scanner.LibraryRefreshInterval`) | `ScannerService.cs` | idem |
| Provider-source connectivity polling | `EmbyService.cs` / `JellyfinService.cs` / `PlexService.cs` | idem |
| Cache cleanup (transcode folder, image cache) | `Application/Maintenance/` | `ersatzrs-infra/src/cache_cleanup.rs` |
| Search-index rebuild dispatcher | `SearchIndexService.cs` | gekoppeld aan E-loop wanneer F-blok klaar is |

### F. Scanner deepening (`crates/ersatzrs-scanner/`) — MEDIUM

Phase 4 is gesloten, maar deze gaten staan nog open:

- **Provider PathInfo tabellen** — `PlexPathInfo`, `JellyfinPathInfo`, `EmbyPathInfo`
  (alle "Schema only" in persistence-gap-matrix). Vereist voor SMB/NFS-mapping
  buiten path-replacement.
- **Etag-based skip** — Provider sync schrijft geen etags terug, dus elke
  scan is een volledige refresh.
- **`MediaChapter`/`MediaStream`/`MediaFile` velden** — basisrijen worden
  geschreven, maar full color/profile/subtitle parity is partieel.
- **Plex collection sync real-world coverage** — alleen mock-tests.
- **Library subtype update/delete** — `JellyfinLibrary`/`EmbyLibrary`/`PlexLibrary`
  bootstrap werkt; volledige update/delete van library-rij + cascade is dun.
- **Music video / Remote stream owner writes** in scanner — momenteel niet
  ondersteund.

### G. Search index hardening (`crates/ersatzrs-core/src/search.rs`) — MEDIUM

ErsatzTV's `LuceneSearchIndex.cs` (1659 LOC) en `ElasticSearchIndex.cs` (1075
LOC) tegenover ~150 LOC SQL-Lucene-evaluator in Rust.

- Director/Writer/Chapter velden zijn placeholders zolang de metadata-joins
  uit Blok C ontbreken.
- Geen analyzer/tokenization parity (geen lowercasing, geen accent-folding,
  geen language-aware stemming).
- Geen fuzzy/proximity/boost/page-map.
- Geen persistente inverted-index.
- Elasticsearch-backend wordt **niet** geport (Dropped via nieuwe ADR — Lucene-
  achtig SQL-pad blijft de canonieke route; tantivy-migratie is een latere
  optimisatie).

### H. Configuratie + ADR's — SMALL

- `FFmpegGlobalWatermarkId` en `FFmpegGlobalFallbackFillerId` `ConfigElement`-keys
  honoreren in playout-build.
- Per-subsystem logging levels (`log.minimum_level.scanning|scheduling|searching|streaming|http`).
- `XmltvBlockBehavior` (Block/Pad/None) — gebruikt door guide-generation;
  guide zit in web-crate maar de behavior moet in core/infra blijven (nu
  verbergt het zich in de transport-laag).
- ADR voor "Classic engine ported / ElasticSearch dropped".

### I. Phase 7 master plan completion — MEDIUM

- Side-by-side benchmark suite (ErsatzRS vs ErsatzTV op identieke channel-config).
- 24-uurs single-channel soak test in CI of dedicated runner.

## Aanbevolen aanpak

Eén volgorde, in slices die elk onafhankelijk te reviewen + mergen zijn. Elke
slice levert een GO/NO-GO-review per de Phase 7-ritme die in STATUS.md is
vastgelegd.

### Slice 11 — `PipelineRequest` source-codec metadata + decoder dispatch
1. ~~Voeg `SourceVideoStream { codec, profile, pixel_format, color_params, scan_kind, framerate }`
   en `SourceAudioStream` toe aan `PipelineRequest` ([crates/ersatzrs-ffmpeg/src/pipeline/mod.rs](crates/ersatzrs-ffmpeg/src/pipeline/mod.rs)).~~
2. ~~Port `FFmpegKnownDecoder.cs` naar een Rust enum + tabel; selecteer per
   `(hwaccel, codec)` de juiste decoder of fall back naar implicit.~~
3. ~~Update alle bestaande snapshot-tests met source-codec metadata in de
   request; bevries argv waar verandering verwacht is.~~

### Slice 12 — Capability detection + `AvailableEncoders/Decoders`
1. ~~Voeg `crates/ersatzrs-ffmpeg/src/capabilities/` toe met `IHardwareCapabilities`-
   tegenhangers per accel (`Nvidia`, `Qsv`, `Vaapi`, `Amf`, `Rkmpp`,
   `Videotoolbox`, `V4l2m2m`).~~
2. ~~Hang ze aan de bestaande Slice 10 startup-runtime report.~~
3. ~~Vervang hardcoded encoder-selectie in `video_encoder()` door een
   factory die capability + source-codec leest.~~
4. ~~Voeg fallback-laddertest met `insta` snapshot.~~

### Slice 13 — Ontbrekende FFmpeg-filters
Volgorde van high-ROI naar low:
1. `NormalizeLoudnessFilter` (loudnorm + aresample two-pass) —
   audio-parity is belangrijk voor live-streaming.
2. Hardware deinterlace per accel (`yadif_cuda`, `deinterlace_qsv`,
   `deinterlace_vaapi`).
3. `TonemapFilter` (+ cuda/qsv/vaapi) — HDR is groeiend.
4. `ColorspaceFilter` (zscale + bt.709 migratie).
5. `HardwareUploadFilter` / `HardwareDownloadFilter` als typed nodes;
   refactor inline hwdownload uit scaling.
6. `FrameRateFilter` met fps-precisie/rounding.

Elke filter krijgt een `insta`-snapshot in de bestaande `ersatzrs-ffmpeg`-
testsuite voor zijn argv-fragment, plus een end-to-end pipeline-snapshot
waar de filter wordt ingevoegd.

### Slice 14 — `ProgramScheduleItem` subtype repositories
1. Schrijf full read/write voor `ProgramScheduleDurationItem`,
   `ProgramScheduleFloodItem`, `ProgramScheduleMultipleItem`, `ProgramScheduleOneItem`
   in [crates/ersatzrs-infra/src/repositories/program_schedule.rs](crates/ersatzrs-infra/src/repositories/program_schedule.rs).
2. Voeg `ProgramScheduleItemWatermark` + `ProgramScheduleItemGraphicsElement`
   join-rijen toe (parallel met de Block-overlay write parity).
3. Voeg `ProgramScheduleAlternate` write toe.
4. Schrijf `BlockItem` typed target round-trip (collection/show/season/
   searchquery FK-velden) in [block.rs](crates/ersatzrs-infra/src/repositories/block.rs).
5. Update `persistence-gap-matrix.md` rij voor rij.

### Slice 15 — Classic scheduling engine
1. Port `Core/Scheduling/ClassicSchedulingContext.cs` + `PlayoutModeSchedulerBase.cs`
   naar `crates/ersatzrs-core/src/scheduling/classic/mod.rs`.
2. Port `PlayoutModeScheduler{One,Multiple,Duration,Flood}.cs` als per-mode
   submodules met `Engine` trait-implementatie.
3. Hang in op de bestaande `PlayoutBuilder`-dispatch
   ([playout_builder.rs](crates/ersatzrs-core/src/playout_builder.rs)) zodat
   `PlayoutScheduleKind::Classic` niet langer faalt.
4. Port `FixedStartTimeBehavior` (Flexible/Fixed/FlexibleOverflow) in de
   schedule-time logica.
5. Voeg ErsatzTV-fixture-based regression-snapshots toe (vergelijkbaar met de
   block scheduling fixtures die er al zijn).

### Slice 16 — Health check engine
1. ~~Voeg `crates/ersatzrs-core/src/health.rs` met `HealthCheck` trait +
   `HealthCheckService` runner.~~
2. ~~Port FFmpegVersion + FFmpegCapabilities check tegen de Slice-10 runtime
   report.~~
3. ~~Port FileNotFound, EmptySchedule, ZeroDuration, MovieMetadata,
   EpisodeMetadata, Unavailable, HardwareAcceleration, VaapiDriver, Downgrade
   in `crates/ersatzrs-infra/src/health.rs`.~~
4. ~~Persisteer resultaten in `ConfigElement` (parity met C#) onder
   `health.results` en `health.summary`.~~
5. Wire de bestaande admin badge / health page naar `health.summary` en de
   gepersisteerde results, of laat de web-laag de engine expliciet draaien
   zodra web-scope weer open is.
6. Voeg een startup/run-once of scheduler-loop toe die de health engine
   automatisch uitvoert.
7. Verdiep HardwareAcceleration/VaapiDriver zodra Slice 12 de typed
   `IHardwareCapabilities`-tegenhanger en per-device probes heeft.

### Slice 17 — Background maintenance loops
1. Maak nieuwe binary `ersatzrs-scheduler` (broeder van `ersatzrs-web` en
   `ersatzrs-scanner`) met de runtime-onafhankelijke loops.
2. Periodic playout reset/build (30 min, respect channel locks).
3. Library refresh dispatcher (`Scanner.LibraryRefreshInterval`).
4. Provider connectivity polling (Plex/Jellyfin/Emby).
5. Cache cleanup (transcode folder, image cache, log rotation).
6. Search-index rebuild dispatcher (gekoppeld aan F-laag wanneer geschikt).

### Slice 18 — Scanner deepening
1. PathInfo tabellen + repository writes (`PlexPathInfo`, `JellyfinPathInfo`,
   `EmbyPathInfo`).
2. Etag-based skip optimisatie in alle drie de provider-sync paden.
3. Real-world Plex collection-sync coverage (env-gated test corpus).
4. Library subtype update/delete + cascade.
5. Music video / Remote stream owner writes (uitbreiden van
   [crates/ersatzrs-scanner/src/lib.rs](crates/ersatzrs-scanner/src/lib.rs)).

### Slice 19 — Search index hardening
1. Director/Writer/Chapter veldindexering zodra Slice C metadata-joins
   landt.
2. Lowercasing + accent-folding analyzer in
   [crates/ersatzrs-core/src/search.rs](crates/ersatzrs-core/src/search.rs).
3. Fuzzy + proximity + boost; page-map voor pagineerbare results.
4. Schrijf nieuwe ADR "Drop ElasticSearch backend".
5. Optionele tantivy-evaluatie als appendix (geen werk).

### Slice 20 — Config + ADR-afronding
1. Honoreer `FFmpegGlobalWatermarkId` + `FFmpegGlobalFallbackFillerId` in
   playout-build.
2. Per-subsystem logging-level keys.
3. `XmltvBlockBehavior` in core (de transport-laag leest de keuze al).
4. ADR "Classic scheduling engine ported".
5. ADR "Drop ElasticSearch backend".

### Slice 21 — Phase 7 closure: benchmarks + soak
1. Side-by-side benchmark harness (ErsatzRS + ErsatzTV op identieke channel-
   config in Docker compose).
2. 24-uurs single-channel soak test runner.
3. STATUS.md + parity-matrix.md updaten naar "Phase 7 complete".

## Critical files

**Aan te passen:**
- [crates/ersatzrs-ffmpeg/src/pipeline/mod.rs](crates/ersatzrs-ffmpeg/src/pipeline/mod.rs) — `PipelineRequest` uitbreiden
- [crates/ersatzrs-ffmpeg/src/filter/](crates/ersatzrs-ffmpeg/src/filter/) — uitbreiden met aparte modules per filter-categorie
- [crates/ersatzrs-core/src/playout_builder.rs](crates/ersatzrs-core/src/playout_builder.rs) — Classic engine inhangen
- [crates/ersatzrs-core/src/scheduling/mod.rs](crates/ersatzrs-core/src/scheduling/mod.rs) — `classic` submodule
- [crates/ersatzrs-core/src/search.rs](crates/ersatzrs-core/src/search.rs) — analyzer + extra velden
- [crates/ersatzrs-core/src/health.rs](crates/ersatzrs-core/src/health.rs) — health contracts + runner
- [crates/ersatzrs-infra/src/health.rs](crates/ersatzrs-infra/src/health.rs) — SQL-backed health checks + `ConfigElement` persistence
- [crates/ersatzrs-infra/src/repositories/program_schedule.rs](crates/ersatzrs-infra/src/repositories/program_schedule.rs) — subtypes + alternates
- [crates/ersatzrs-infra/src/repositories/block.rs](crates/ersatzrs-infra/src/repositories/block.rs) — typed target round-trip
- [crates/ersatzrs-infra/src/repositories/metadata.rs](crates/ersatzrs-infra/src/repositories/metadata.rs) — search-join schrijfpaden
- [crates/ersatzrs-infra/src/{plex,jellyfin,emby}.rs](crates/ersatzrs-infra/src/) — etags + PathInfo
- [crates/ersatzrs-scanner/src/lib.rs](crates/ersatzrs-scanner/src/lib.rs) — extra owner writes
- [docs/reference/persistence-gap-matrix.md](docs/reference/persistence-gap-matrix.md), [docs/reference/parity-matrix.md](docs/reference/parity-matrix.md), [STATUS.md](STATUS.md)

**Nieuw aan te maken:**
- `crates/ersatzrs-ffmpeg/src/capabilities/{mod,nvidia,qsv,vaapi,amf,rkmpp,videotoolbox,v4l2m2m}.rs`
- `crates/ersatzrs-ffmpeg/src/decoder/{mod,cuvid,qsv,v4l2m2m,vaapi,videotoolbox,rkmpp,implicit}.rs`
- `crates/ersatzrs-ffmpeg/src/encoder/{mod,available,nvenc,qsv,amf,vaapi,rkmpp,v4l2m2m,videotoolbox,libx264,libx265,…}.rs`
- `crates/ersatzrs-core/src/scheduling/classic/{mod,context,one,multiple,duration,flood,base}.rs`
- `crates/ersatzrs-core/src/health/{mod,check,result}.rs`
- `crates/ersatzrs-core/src/maintenance/mod.rs`
- `crates/ersatzrs-infra/src/health/{mod,…per-check}.rs`
- `crates/ersatzrs-infra/src/cache_cleanup.rs`
- `crates/ersatzrs-scheduler/{Cargo.toml,src/main.rs,src/lib.rs}` — nieuwe binary
- `migrations/0003_*.sql` + `migrations/mysql/0003_*.sql` als provider PathInfo of nieuwe joins schemavraag hebben (waarschijnlijk niet nodig — schema bestaat al)
- `docs/adr/0009-classic-scheduling-engine-ported.md`
- `docs/adr/0010-drop-elasticsearch-backend.md`

## Hergebruik

- `BlockSchedulingEngine` (in [crates/ersatzrs-core/src/scheduling/block.rs](crates/ersatzrs-core/src/scheduling/block.rs)) is het patroon voor de Classic engine — zelfde shared trait + content-resolver model.
- De Slice-10 FFmpeg runtime report ([crates/ersatzrs-ffmpeg/src/version.rs](crates/ersatzrs-ffmpeg/src/version.rs)) is de input voor de capability-laag — niet vanaf nul opnieuw probe-en.
- `PlayoutBuilder` in [crates/ersatzrs-core/src/playout_builder.rs](crates/ersatzrs-core/src/playout_builder.rs) heeft al `ExternalJsonSchedulingEngine` als no-op patroon — nieuwe Classic engine volgt zelfde dispatch-vorm.
- `apply_build_result` in `playout.rs` schrijft al gaps/history/items — Classic engine moet hetzelfde resultaat-type produceren zodat de persistence-laag onveranderd blijft.
- `count_health_issues` query in [crates/ersatzrs-infra/src/repositories/](crates/ersatzrs-infra/src/repositories/) is de bestaande lezer voor de admin-badge — nieuwe checks moeten in dezelfde tabel schrijven.
- `TraktSynchronizer` in [crates/ersatzrs-infra/src/trakt_sync.rs](crates/ersatzrs-infra/src/trakt_sync.rs) is het loop-patroon dat Slice 17 kan kopiëren voor playout/scan/cache loops.
- `insta`-snapshots in `crates/ersatzrs-ffmpeg/src/snapshots/` en `crates/ersatzrs-core/src/scheduling/snapshots/` zijn het regressie-gate-patroon — elke nieuwe filter / scheduler-mode krijgt zijn eigen snapshot-set.

## Verificatie

Per slice:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features` (incl. `cargo insta review` waar argv-snapshots wijzigen)
- `cargo deny check`

End-to-end (op afsluiting van Phase 7):
- ErsatzTV en ErsatzRS draaien een identieke channel-config (Block + YAML +
  Classic + Scripted) en produceren PlayoutItem-rijen met dezelfde
  start/finish-tijden binnen tolerantie.
- 24-uurs soak: één channel via `/iptv/channel/{n}.ts` zonder geheugen-leak,
  ffmpeg-restart-storm, of gap-skip.
- `cargo run --bin ersatzrs-scheduler` draait alle loops zonder paniek;
  health-check resultaten verschijnen in `ConfigElement` en de admin badge.
- Geïmporteerde ErsatzTV SQLite-database met Classic schedules bouwt een
  geldige playout via `PlayoutBuilder::default().build(Classic)`.
- `cargo run --bin ersatzrs-scanner -- --provider plex --library-id N` gebruikt
  etag-skip op een tweede run (zichtbaar in tracing).
- `/healthz` blijft 200 onder volle worker-load.
