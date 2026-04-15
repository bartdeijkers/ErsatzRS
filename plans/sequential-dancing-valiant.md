# NasTv — Rust/FFmpeg 8.1 herimplementatie van ErsatzTV

## Context

De gebruiker wil [ErsatzTV](/home/bart/ErsatzTV/) nabouwen in Rust met FFmpeg 8.1 als fundament voor de graphics engine. ErsatzTV is een ~131K LOC .NET 10 mediaserver die persoonlijke media (Plex/Jellyfin/Emby) omzet in lineaire TV-kanalen met IPTV/HDHR streaming en hardware-accelerated transcoding. Doel is volledige feature-parity op termijn, beginnend breed (alle pijlers vanaf dag 1), in nieuwe lege map [/home/bart/NasTv](/home/bart/nastv/).

**Waarom:** .NET/Blazor stack vervangen door één statisch Rust-binary + moderne FFmpeg 8.1 features (m.n. `drawvg` vector graphics filter) voor een eenvoudiger deployment en rijkere in-stream graphics dan ErsatzTV zelf biedt.

**Beslissingen (uit vragen):**
- UI: **Leptos** (full-stack Rust, SSR + hydration)
- FFmpeg: **CLI spawn** via `tokio::process`, net als ErsatzTV (geen libav bindings)
- Scope: **volledige parity op termijn, breed beginnen**
- Directory: `/home/bart/NasTv`

---

## Tech stack: is dit goed genoeg?

**Conclusie: ja, compleet en geschikt — met één kanttekening over de UI-leercurve.**

### FFmpeg 8.1 specifiek

| Feature | Relevantie voor NasTv |
|---|---|
| `drawvg` filter (libcairo, SVG-achtige vectors) | **Kernwinst**: vervangt custom graphics engine van ErsatzTV (`ScriptGraphicsElement` + `OverlayGraphicsEngineFilter`). Channel logos, progress bars, lower thirds, EPG-style overlays direct in filter graph. |
| Vulkan-accelerated swscale | Gratis speed-up voor scale/color conversion, zonder NVENC/VAAPI vendor lock-in. |
| D3D12 filters (`scale_d3d12`, `deinterlace_d3d12`, `mestimate_d3d12`) | Windows hardware pad naast bestaande NVENC/QSV/AMF. |
| JPEG-XS encode/decode, xHE-AAC, MPEG-H 3D audio | Niet MVP-kritiek, wel future-proof. |
| EXIF parsing | Handig voor still-image kanalen (foto-slideshows). |

ErsatzTV's FFmpeg-pipeline is versie-agnostisch en hergebruik van hun filter-patronen (watermark fade, subtitle overlay, concat/resetpts, tonemap) is direct bruikbaar — FFmpeg 8.1 breekt hier niets.

### Rust ecosystem — alle gaten gedekt

| Behoefte (ErsatzTV component) | Rust crate | Notitie |
|---|---|---|
| ASP.NET Core host | `axum` + `tower` + `tokio` | |
| Blazor/MudBlazor UI | `leptos` + `leptos_axum` + `leptos_router` | SSR+hydration; MudBlazor componenten zelf bouwen of [thaw-ui](https://github.com/thaw-ui/thaw) overwegen |
| MediatR (CQRS) | geen crate nodig — plain `async fn` handlers per use-case, traits voor repositories | |
| EF Core (SQLite + MySQL) | `sqlx` met runtime-feature selectie | `sqlx migrate` voor schema; ondersteunt beide backends in één crate |
| LanguageExt `Option<T>`/`Either<L,R>` | `Option`/`Result` is native; `anyhow`/`thiserror` voor errors | |
| FFmpeg CLI spawn (`CliWrap`) | `tokio::process::Command` | |
| CLI argument builder | eigen builder module (port van `ErsatzTV.FFmpeg/CommandGenerator.cs`) | |
| Lua scripting (`ScriptedScheduling`) | `mlua` (Lua 5.4) | volwassen, async-ondersteuning |
| YAML parsing (`YamlScheduling`) | `serde_yaml` of `serde_yml` | |
| Plex/Jellyfin/Emby HTTP clients | `reqwest` + `serde` DTOs | handmatig porten uit [ErsatzTV.Infrastructure](/home/bart/ErsatzTV/ErsatzTV.Infrastructure/) |
| HLS muxer/segmentatie | delegeren aan FFmpeg (geen eigen code) | |
| HDHR UDP discovery (port 65001) | `tokio::net::UdpSocket` | |
| Background scanner (`ErsatzTV.Scanner`) | aparte binary in werkspace, deelt `core` crate | |
| Health/logging | `tracing` + `tracing-subscriber` | |
| Config | `figment` of `config` + `serde` | |
| Testing | `tokio::test` + `rstest` + `insta` (snapshot) | |

### Openstaande risico's
1. **Leptos-leercurve** — signals/reactive model is anders dan Blazor. Tip: begin UI als server-rendered Leptos (`ssr`-feature) zonder hydratie en voeg interactiviteit pas toe waar nodig.
2. **MudBlazor component library** heeft geen 1:1 equivalent. Plan extra tijd voor eigen component-laag (forms, tables, datepickers) of adopteer `thaw`/`leptonic`.
3. **MySQL + SQLite dual-support** vereist óf runtime-dispatch óf compile-time feature flags in `sqlx`. ErsatzTV lost dit met twee Infrastructure-projecten op — in Rust kan het met features in één crate.
4. **393 EF migraties** zijn niet 1:1 over te nemen. Start met een gecondenseerd initieel schema afgeleid uit [TvContext.cs](/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs) (99 DbSets). Migraties vanaf daar.

---

## Architectuur: crate-indeling

Cargo workspace in `/home/bart/nastv/`:

```
NasTv/
├── Cargo.toml              # workspace root
├── crates/
│   ├── nastv-domain/       # entities, enums, value objects (port ErsatzTV.Core/Domain)
│   ├── nastv-core/         # business logic, scheduling engines, FFmpeg pipeline abstractie
│   │                       # (port ErsatzTV.Core zonder Domain)
│   ├── nastv-ffmpeg/       # FFmpeg command builder, pipeline builders per HW-pad
│   │                       # (port ErsatzTV.FFmpeg)
│   ├── nastv-infra/        # sqlx repositories, migrations, HTTP clients
│   │                       # (port ErsatzTV.Infrastructure)
│   ├── nastv-app/          # use-case handlers (port ErsatzTV.Application)
│   ├── nastv-web/          # axum + leptos host, API routes, IPTV/HDHR endpoints
│   │                       # (port ErsatzTV/)
│   └── nastv-scanner/      # scanner binary (port ErsatzTV.Scanner)
├── migrations/             # sqlx migrations (shared tussen SQLite/MySQL waar mogelijk)
└── assets/                 # templates, SVG assets voor drawvg overlays
```

Binaries: `nastv-web` (server) en `nastv-scanner` (CLI). `nastv-core` / `nastv-ffmpeg` zijn de zwaarste crates.

---

## Fase-indeling (breed beginnen, parity als eindpunt)

### Fase 0 — Bootstrap (week 1)
- Cargo workspace opzetten met alle crates als lege stubs
- `rustfmt.toml`, `clippy` config, `cargo-deny`, CI via GitHub Actions
- FFmpeg 8.1 runtime check: detecteer versie, fail fast bij <8.1 (voor `drawvg`)
- Docker image met `ffmpeg:8.1` + nastv-web binary
- Minimale `axum` + `leptos` hello-world met 1 SSR pagina
- `sqlx` database bootstrap (SQLite eerst, MySQL feature-flag later)

### Fase 1 — Domein + persistentie (week 2–4)
- Port entities uit [ErsatzTV.Core/Domain/](/home/bart/ErsatzTV/ErsatzTV.Core/Domain/) naar `nastv-domain`:
  - `Channel`, `Playout`, `PlayoutItem`, `ProgramSchedule`, `ProgramScheduleItem` (varianten)
  - `MediaItem` hierarchie: Movie, Episode, Show, Season, Song, MusicVideo, OtherVideo, Image, RemoteStream
  - `MediaVersion`, `MediaFile`, `MediaStream`, `MediaChapter`
  - `Collection` (+ Smart/Multi/Rerun varianten), `Playlist`, `Block`, `BlockItem`
  - `Library` + `MediaSource` (Plex/Jellyfin/Emby/Local)
  - `FFmpegProfile`, `Watermark`, `GraphicsElement`, `Deco*`, `Artwork`, `FillerPreset`
- Eerste sqlx schema migratie — geen 1:1 kopie van 393 EF-migraties; één schema samengevoegd uit de finale state van [TvContext.cs](/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs)
- Repository traits in `nastv-core`, sqlx implementaties in `nastv-infra`
- Unit tests voor entity-mapping

### Fase 2 — FFmpeg pipeline + drawvg graphics engine (week 4–8)
- Port [ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs](/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs) en [CommandGenerator.cs](/home/bart/ErsatzTV/ErsatzTV.FFmpeg/CommandGenerator.cs) als Rust builder pattern
- Begin met `SoftwarePipelineBuilder` (referentie-implementatie), dan NVENC en VAAPI
- Port [ComplexFilter.cs](/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs): label-routing voor video/audio/watermark/subtitle/graphics
- Port watermark/subtitle/song-progress filters
- **Nieuw**: `drawvg`-gebaseerde graphics engine met SVG-templates (vervangt ErsatzTV's `ScriptGraphicsElement` + externe renderer pipe)
  - SVG templates in `assets/` met `{{variables}}` voor dynamische content (titels, tijd, progress %)
  - Renderer die template → SVG string → `drawvg=source=...` filter genereert
- Port `ConcatPlaylist.cs` → ffconcat v1.0 writer, `resetpts` handling
- Snapshot-tests op gegenereerde FFmpeg-argumenten (zoals ErsatzTV.FFmpeg.Tests doet)

### Fase 3 — Scheduling engines (week 6–10, deels parallel met fase 2)
Alle drie engines porten uit [ErsatzTV.Core/Scheduling/](/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/):
1. **Block scheduling** — `BlockPlayoutBuilder.cs`, `BlockPlayoutEnumerator.cs`
2. **YAML scheduling** — 50+ instruction handlers (Duration, Count, PadToNext, PreRoll, PostRoll, GraphicsOn/Off, Watermark, etc.); `serde_yaml` deserialisatie + trait-dispatch per instruction
3. **Scripted scheduling** — Lua runtime via `mlua`; expose scheduling API aan scripts

Gedeelde `ISchedulingEngine`-trait (fluent builder) naar Rust trait + struct. Playout state machine met tijd-anchors en gap-handling.

### Fase 4 — Media source integraties (week 8–12)
Port HTTP clients uit [ErsatzTV.Infrastructure/Plex/](/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Plex/), `Jellyfin/`, `Emby/`:
- `reqwest::Client` + `serde` DTOs per provider
- Path-replacement logica (lokaal vs. remote paden)
- Incremental sync via etags
- `nastv-scanner` binary die deze clients aanroept, lokale filesystem scant, `ffprobe` invoked voor metadata

### Fase 5 — Protocol endpoints (week 10–13)
- `/iptv/channels.m3u` — M3U playlist generator
- `/iptv/video` — HLS proxy endpoint met auth filters
- HDHR: `/discover.json`, `/device.xml`, `/lineup.json`, `/lineup_status.json` + UDP discovery broadcaster op port 65001
- `/guide` — EPG/XMLTV output
- Port [ErsatzTV/Controllers/](/home/bart/ErsatzTV/ErsatzTV/) als axum routers

### Fase 6 — Leptos admin UI (week 12–20, zwaarste fase)
Port de 132 Razor componenten stapsgewijs; prioriteit:
1. Channel lijst/editor
2. Media source config (Plex/Jellyfin/Emby/Local)
3. Library browser + collections
4. Block editor
5. YAML playout editor (met schema-validatie)
6. FFmpeg profile editor
7. Graphics/watermark editor (met live drawvg preview!)
8. Playout visualizer/timeline

Start server-rendered, voeg islands-hydratie toe waar interactiviteit nodig is.

### Fase 7 — Hardening & parity-completering (lopend)
- Hardware acceleratie: QSV, AMF, VideoToolbox, RKMPP, V4L2M2M (na NVENC/VAAPI in fase 2)
- MySQL backend activeren naast SQLite
- Trakt integratie, GitHub version check
- Docker multi-arch images
- Performance-tests tegen ErsatzTV baseline

---

## Kritieke bestanden om te raadplegen bij het porten

| Doel | ErsatzTV bron |
|---|---|
| Pipeline builder patroon | [ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs](/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs), [SoftwarePipelineBuilder.cs](/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Pipeline/SoftwarePipelineBuilder.cs) |
| Filter complex generatie | [ErsatzTV.FFmpeg/Filter/ComplexFilter.cs](/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs) |
| Watermark/overlay filters | [ErsatzTV.FFmpeg/Filter/OverlayWatermarkFilter.cs](/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/OverlayWatermarkFilter.cs), [SongProgressFilter.cs](/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/SongProgressFilter.cs) |
| FFmpeg argv assemblage | [ErsatzTV.FFmpeg/CommandGenerator.cs](/home/bart/ErsatzTV/ErsatzTV.FFmpeg/CommandGenerator.cs) |
| Process spawning | [ErsatzTV.Core/FFmpeg/FFmpegLibraryProcessService.cs](/home/bart/ErsatzTV/ErsatzTV.Core/FFmpeg/FFmpegLibraryProcessService.cs) |
| Seamless concat | [ErsatzTV.Core/FFmpeg/ConcatPlaylist.cs](/home/bart/ErsatzTV/ErsatzTV.Core/FFmpeg/ConcatPlaylist.cs) |
| DB schema (99 DbSets) | [ErsatzTV.Infrastructure/Data/TvContext.cs](/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs) |
| Scheduling engines | [ErsatzTV.Core/Scheduling/](/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/) (BlockScheduling/, YamlScheduling/, ScriptedScheduling/, Engine/) |
| HLS output config | [ErsatzTV.FFmpeg/OutputFormat/OutputFormatHls.cs](/home/bart/ErsatzTV/ErsatzTV.FFmpeg/OutputFormat/OutputFormatHls.cs) |
| HDHR markdown spec (intern) | [/home/bart/ErsatzTV/hdhr.md](/home/bart/ErsatzTV/hdhr.md) |
| Plex/Jellyfin/Emby clients | [ErsatzTV.Infrastructure/Plex/](/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Plex/), `Jellyfin/`, `Emby/` |
| Domein referentie | [ErsatzTV.Core/Domain/](/home/bart/ErsatzTV/ErsatzTV.Core/Domain/) |

---

## Verificatie

**Fase 0:**
- `cargo build --workspace` groen
- `curl localhost:8080/healthz` → 200
- `ffmpeg -version` in container toont 8.1.x
- SQLite DB file wordt aangemaakt op startup

**Fase 1:**
- Alle entities hebben een sqlx-roundtrip test (insert → select → equals)
- Schema-migratie draait idempotent op lege DB en op gestarte DB
- `cargo test -p nastv-domain -p nastv-infra` groen

**Fase 2:**
- Snapshot-tests op FFmpeg argv output voor software + NVENC paden
- End-to-end: voer `nastv-web` lokaal uit, speel een lokaal MP4-bestand af via `/iptv/video/<channel>`, open in VLC, controleer seamless afspelen
- `drawvg` overlay: render een SVG-template op video, verifieer visueel in VLC
- Watermark fade-in/out: verifieer in VLC

**Fase 3:**
- Unit tests per YAML instruction handler (input YAML → verwachte playout timeline)
- Block scheduling: vergelijk gegenereerde timeline met ErsatzTV output voor identieke input (als mogelijk)
- Lua script: draai een scripted playout die media roteert, verifieer correcte volgorde

**Fase 4:**
- Integration test: spin up Jellyfin container, scan library, verifieer dat items in DB staan
- Pad-replacement werkt voor lokaal/docker scenario
- Idem Plex en Emby

**Fase 5:**
- `/iptv/channels.m3u` laden in VLC → kanalen verschijnen
- HDHR: Plex DVR kan device discovery doen (UDP 65001)
- `/guide` → geldige XMLTV die Jellyfin/Plex EPG kan parsen

**Fase 6:**
- Handmatige UI-doorloop: maak kanaal aan, configureer mediabron, bouw playout, start stream — allemaal via Leptos UI
- Geen browser-console errors op kritieke paden

**Fase 7 / doorlopend:**
- Side-by-side benchmark tegen ErsatzTV: dezelfde channel config, meet CPU/memory/startup-time
- 24-uurs soak test op één kanaal zonder crash of memory leak

---

## Bronnen

- [FFmpeg 8.1 release notes (Phoronix)](https://www.phoronix.com/news/FFmpeg-8.0-Released)
- [FFmpeg 8.1 Feature Highlights](https://www.the-automator.com/ffmpeg-8-1-feature-highlights-and-implications-to-our-wrappers/)
- [FFmpeg 8.1 Ubuntu PPA + Rockchip/JPEG-XS](https://ubuntuhandbook.org/index.php/2026/03/ffmpeg-8-1-rockchip-jpeg-xs/)
- [rusty_ffmpeg 0.17.0+ffmpeg.8.1](https://crates.io/crates/rsmpeg) (voor eventuele toekomstige libav-hybride)
- [rsmpeg on lib.rs](https://lib.rs/crates/rsmpeg)
