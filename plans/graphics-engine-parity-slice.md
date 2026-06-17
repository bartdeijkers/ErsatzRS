# Graphics-Engine Streaming Parity (drawvg)

> **Status: Completed / historical.** Shipped (`drawvg-graphics-engine` is
> `Ported`). Kept for context; live state lives in `docs/progress/`.

## Context

ErsatzTV's **graphics engine** overlays custom graphics elements on top of the primary
content "with a high degree of control and customization"
([docs](https://ersatztv.org/docs/advanced/graphics-engine/)). It supports four element
types — **Image**, **Text**, **Subtitle**, **Motion** — defined as YAML files
(pre-processed with Scriban templates) under
`<config>/templates/graphics-elements/{image,text,subtitle,motion}/`, and attached to
playback through classic / block / sequential / scripted schedules.

In **ErsatzRS** the scaffolding largely exists but is **never wired into streaming**:

- Domain model `GraphicsElement` (kinds `Image, Text, Subtitle, Motion, Script`) plus the
  join tables `PlayoutItemGraphicsElement` (with per-item `Variables`),
  `ProgramScheduleItemGraphicsElement`, `BlockItemGraphicsElement`, `DecoGraphicsElement`
  all exist (`crates/ersatzrs-domain/src/graphics.rs`, `migrations/0001_initial.sql`). `Deco`
  carries `GraphicsElementsMode` + `UseGraphicsElementsDuringFiller`.
- `GraphicsRepository` (`get_element`, `list_for_channel`, `upsert_element`) is implemented
  (`crates/ersatzrs-infra/src/repositories/graphics.rs`).
- The drawvg primitives exist: `DrawvgOverlay`, `GraphicsTemplateRenderer`, `SvgTemplate`
  (`{{var}}` substitution) in `crates/ersatzrs-ffmpeg/src/graphics/mod.rs`, and the pipeline
  already wires `request.drawvg_filters` → `graphics_overlay` + a lavfi dummy input
  (`crates/ersatzrs-ffmpeg/src/pipeline/mod.rs:714-720`).
- Scripted schedules already expose `graphics_on`/`graphics_off` (by id) and
  `graphics_on_path`/`graphics_off_path` (by path + variables JSON)
  (`crates/ersatzrs-core/src/scheduling/scripted.rs`).

**The gap:**
- The streaming command builders never load or apply graphics elements —
  `request.drawvg_filters` is only ever populated by tests, and `graphics_element_ids` is
  read only by the schedule editor.
- There is **no per-kind rendering** (image / text / subtitle / motion). Only a single
  source SVG fixture ships: `assets/graphics/lower-third.svg`.
- Runtime template variables (channel/content time, EPG metadata, `MediaItem_Duration`) and
  the per-item `Variables` overrides are not resolved or substituted at stream-build time.

**This plan was split out of the original `watermark-parity-slice.md`** (since
shipped and removed; watermarks are `watermark-editor`, Ported) so the watermark
fix (channel-logo display) could ship independently. Watermarks use native FFmpeg
overlay filters; this plan owns the general graphics-element engine.

**Outcome:** image/text/subtitle/motion graphics elements render in the final video output
for IPTV (`.ts`/MPEG-TS), HLS, and the dev stream, attached through every schedule type,
faithfully porting ErsatzTV's graphics engine within the drawvg architecture (ADR 0005, no
external graphics process).

### Locked / governing decisions

- **ADR 0005 (drawvg):** the graphics engine is FFmpeg 8.1 `drawvg` + VGS templates in
  `assets/`; **do not reintroduce an external graphics process or feed raw SVG/XML to drawvg.**
- **Storage (Decision C — confirmed):** the DB is the canonical store for graphics elements;
  an ErsatzTV-style YAML file at `Path` is parsed to **upsert** a fully-populated element into
  the DB for porting/compatibility. See the domain-model section.
- **Rendering split (refinement to confirm — see domain-model section):** vector/text kinds
  (`Text`, `Subtitle`, `Script`) render via `drawvg` (and libass for `.ass` subtitle
  templates); raster/video kinds (`Image`, `Motion`) render via native FFmpeg `overlay`
  (looped image / looped-seeked video input), consistent with the watermark plan's
  "native FFmpeg for raster/video, drawvg for vector" principle.

### ErsatzTV element model (authoritative field set, from `ErsatzTV.Core/Graphics/*.cs`)

These fields are deserialized from the YAML file at `GraphicsElement.Path` (see the
domain-model finding below), not stored in the DB.

| Kind | Configuration fields |
| --- | --- |
| **Image** | `image` (path), `opacity_percent`, `opacity_expression`, `location` (`WatermarkLocation`), `horizontal_margin_percent`, `vertical_margin_percent`, `location_x`, `location_y`, `z_index`, `scale`, `scale_width_percent`, `place_within_source_content` |
| **Text** | `text`, `opacity_percent`, `opacity_expression`, `location`, `horizontal_margin_percent`, `vertical_margin_percent`, `width_percent`, `text_fit` (`None`/`Wrap`/`Scale`), `text_align` (`Left`/`Center`/`Right`), `z_index`, `base_style`, `include_fonts_from`, `epg_entries`, and a `styles` list — each `StyleDefinition`: `name`, `font_family`, `font_size`, `font_weight`, `font_italic`, `text_color`, `letter_spacing`, `line_height`, `halo_color`, `halo_width`, `halo_blur` |
| **Subtitle** | `z_index`, `epg_entries`, `template` (path to `.ass` file) |
| **Motion** | `video_path`, `location`, `horizontal_margin_percent`, `vertical_margin_percent`, `z_index`, `epg_entries`, `scale`, `scale_width_percent`, `start_seconds`, `end_behavior` (`Disappear` default / `loop`), `hold_seconds` |
| **Script** | `command` + `args` (**external process** — see ADR-0005 note below), `z_index`, `epg_entries`, `start_seconds`, `duration_seconds`, `pixel_format`, `format` |

All kinds share `name` (`BaseGraphicsElement`). `location` reuses the watermark
`WatermarkLocation` enum, so the watermark plan's location→overlay-xy mapping is directly
reusable.

**Integration points** (docs): Classic schedules (per schedule item), Block schedules (per
block item and per deco), Sequential schedules (`Graphics On` / `Graphics Off`
instructions), Scripted schedules (`graphics_on` / `graphics_off`).

**Template variables** (docs): Scriban templates with variable substitution; optional EPG
entry data for text replacement; Motion elements can reference
`MediaItem_Duration.TotalSeconds` (e.g. `start_seconds: {{ MediaItem_Duration.TotalSeconds - 5 }}`).

### ErsatzTV reference files (read before porting each piece)

- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Streaming/Graphics/GraphicsEngine.cs` — frame loop,
  z-order, element initialization.
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Streaming/Graphics/{Image,Text,Subtitle,Motion}/`
  element implementations.
- `$ERSATZTV_SRC/ErsatzTV.Core/Interfaces/Streaming/GraphicsEngineContext.cs` — runtime
  context (frame size, frame rate, channel/content start, seek, duration, template
  variables).
- `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Filter/OverlayGraphicsEngineFilter.cs` — overlay/format.

---

## Phase 1 — Graphics-element data model & YAML importer (Decision C)

DB is the canonical store; an ErsatzTV YAML file upserts into it. See the domain-model
decision below.

Status: implemented for the data/repository/importer slice. Rendering and streaming
integration remain in later phases.

- [x] **Schema (`migration 0003`):** replace the half-built `0002` geometry columns with the real
  model — shared columns (`Kind`, `Name`, `Path`, `ZIndex`, `OpacityPercent`,
  `OpacityExpression`, `Location`, `HorizontalMarginPercent`, `VerticalMarginPercent`,
  `LocationX`, `LocationY`, `Scale`, `ScaleWidthPercent`, `PlaceWithinSourceContent`,
  `EpgEntries`) plus a JSON `Config` column for kind-specific extras (Text `styles`/`text`/
  `text_fit`/`text_align`/`width_percent`/`base_style`/`include_fonts_from`; Motion `video_path`/
  `start_seconds`/`hold_seconds`/`end_behavior`; Subtitle `template`; Script `command`/`args`/
  `format`/`pixel_format`/`duration_seconds`). Add the matching `migrations/mysql/0003`. Follow
  the EF-style `PK_/FK_/IX_` and identifier-casing rules.
- [x] **Domain + repository:** reshape `GraphicsElement` (`crates/ersatzrs-domain/src/graphics.rs`)
  to the new fields (drop the never-persisted `x/y/width/height`/`script`/`variables` phantoms),
  update `map_element` / `upsert_element` (`repositories/graphics.rs`) and the existing
  repository tests (`graphics_repository.rs`, `deco_repository.rs`). `Config = NULL` now decodes
  to the default for the row kind, and malformed or kind-mismatched JSON fails instead of being
  silently coerced.
- [x] **YAML importer:** a loader that parses the ErsatzTV YAML (honor the `YamlMember` aliases from
  `$ERSATZTV_SRC/ErsatzTV.Core/Graphics/*GraphicsElement.cs`) into the domain model and calls
  `upsert_element`, keeping `Path` as the source link. Expose an import entry point — at minimum
  a templates-folder sync; an admin "import" action can follow. Use `serde_yaml` (already in the
  stack for the YAML schedule engine). `graphics_import.rs` now parses one document by kind and
  upserts it through `GraphicsRepository`; folder sync/admin import can build on this entry point.
- [x] Text/`template` strings keep their `{{ }}` placeholders in the DB (resolved at render time in
  Phase 3).

## Phase 2 — Resolve & load graphics elements for streaming

Status: implemented for playback resolution/loading. Streaming builders still need
to consume the resolved elements in Phase 5.

Mirror the subtitle/watermark resolver pattern (`select_playback_subtitle` in
`crates/ersatzrs-infra/src/playback.rs`).

- [x] Add `select_playback_graphics(pool, channel, active_item) -> Vec<ResolvedGraphicsElement>`
  collecting elements from all sources with precedence: deco (`DecoGraphicsElement`) → block
  item (`BlockItemGraphicsElement`) → classic schedule item
  (`ProgramScheduleItemGraphicsElement`) → playout item (`PlayoutItemGraphicsElement`).
  Reuse `GraphicsRepository::list_for_channel` / `get_element`. The resolver consumes the
  materialized `PlayoutItemGraphicsElement` rows produced by classic/block/sequential/scripted
  build paths; direct schedule joins remain Phase 6 build-integration proof.
- [x] Honor `Deco.GraphicsElementsMode` and `UseGraphicsElementsDuringFiller`, and any
  per-element enable window (`enable_expression` / scripted `graphics_on`/`graphics_off`).
  The current DB shape has no separate graphics enable-expression column after Decision C;
  scripted on/off state is represented by the persisted playout-item join.
- [x] Load per-item `Variables` (`PlayoutItemGraphicsElement.Variables`) and element-level
  `variables` for substitution in Phase 3. Element-level variables are not a Decision C
  field; per-item variables are carried.
- [x] Extend the active-item query (`transport/iptv/stream.rs:load_active_media_path`) to surface
  the active `PlayoutItem` id (shared with the watermark plan) so element joins resolve.

## Phase 3 — Template + variable substitution (Scriban parity)

Status: implemented for the render-time variable model and placeholder/expression
substitution helper. Per-kind renderers still need to consume it in Phase 4.

- [x] Assemble a runtime variable set from `GraphicsEngineContext` equivalents: channel time,
  content time, content total duration (`MediaItem_Duration`), frame size/rate, seek, and EPG
  entry fields for the active media item.
- [x] Merge variable sources in precedence order: element `variables` < per-item `Variables` <
  runtime variables. Decision C removed element-level variables, so the implemented order is
  runtime/media/EPG variables followed by per-item `Variables` overrides.
- [x] Extend `SvgTemplate`/`GraphicsTemplateRenderer` (`graphics/mod.rs`). Decide the Scriban
  equivalent (see Open Decisions) — at minimum keep `{{ name }}` substitution; add the small
  expression support needed for Motion `start_seconds` arithmetic and `epg_entries`.
  `graphics_template.rs` now renders `{{ ... }}` placeholders with dotted/indexed lookup
  (`Epg[0].Title`, `Resolution.Width`) and evaluates numeric `+`/`-` expressions such as
  `MediaItem_Duration.TotalSeconds - 5`; Phase 4 will thread it into text/subtitle/motion
  rendering.

## Phase 4 — Per-kind rendering

- Status: started. `crates/ersatzrs-ffmpeg/src/graphics/mod.rs` now renders
  DB-backed Image/Text elements into a split render plan: Image elements become
  looped native `graphics_image` inputs plus overlay metadata, and Text elements
  become deterministic full-frame VGS draw scripts plus `DrawvgOverlay` filters.
  `PipelineRequest::graphics_overlays` interleaves native image overlays and
  drawvg filters by `z_index`; `FFmpegPipeline` carries finalized generated
  inputs so emitted commands include the graphics sources they reference.
  Snapshot coverage lives in
  `crates/ersatzrs-ffmpeg/tests/pipeline_snapshots.rs`
  (`graphics_element_renderer_writes_drawvg_scripts_snapshot`,
  `graphics_element_renderer_writes_script_drawvg_snapshot`,
  `pipeline_interleaves_native_and_drawvg_graphics_by_z_index_snapshot`,
  `pipeline_with_looped_motion_graphics_snapshot`,
  `pipeline_with_disappearing_motion_graphics_snapshot`,
  `pipeline_with_hold_motion_graphics_snapshot`,
  `pipeline_with_subtitle_graphics_overlay_snapshot`).
- [x] **Image** → native FFmpeg `overlay` (looped still image input + scale/location/margins/
  opacity/z-index/enable). Reuses the watermark overlay helpers in `filter/simple.rs`
  (`watermark_overlay_filter`, `watermark_scale_filter`, opacity). Streaming
  builders still need DB/render-plan plumbing in Phase 5.
- [x] **Text** → generate a `drawvg` VGS script from `text` + `styles` (font family/size/color/weight/
  italic/letter spacing, `base_style`), honoring `include_fonts_from`; overlay via
  `DrawvgOverlay`.
- [x] **Subtitle** → libass `.ass` template via the `ass`/`subtitles` filter
  (reuse the existing subtitle filter path), with EPG substitution applied to
  the template first. The FFmpeg render-plan layer writes a rendered `.ass`
  artifact, creates a transparent lavfi `color=black@0.0,...,subtitles=...`
  graphics overlay input with `alpha=1`, and composites it by `z_index`.
- [x] **Motion** → looped/seeked video input + `overlay` (`start_seconds`,
  `end_behavior=loop`, transparent or opaque); chroma/alpha handled per
  source. The FFmpeg render-plan layer emits native motion inputs with optional
  `-stream_loop -1`, `setpts=PTS+start/TB`, scaled overlay filters, start-time
  `enable`, and `eof_action=pass` for Disappear. Hold behavior now probes the
  source duration in infra, falls back to zero duration when probing fails like
  upstream, and emits a bounded native overlay enable window for
  `start_seconds + source_duration + hold_seconds` while preserving FFmpeg's
  repeated last frame.
- [x] **Script** → in-process Lua graphics producing a drawvg script; `command` is
  interpreted as a Lua file path or inline Lua source, `args` and render-time
  variables are exposed to Lua, and the returned VGS script is written as a
  deterministic drawvg artifact with `start_seconds` / `duration_seconds`
  enable timing. ErsatzRS never spawns the ErsatzTV external graphics process.
- Composite all elements (and the watermark) in ascending `z_index`.

## Phase 5 — Wire graphics into streaming

Mirror the watermark plumbing in all three builders:

- `transport/iptv/stream.rs` `serve_channel_stream` / `build_stream_command`.
- `infra/hls_worker.rs` `build_hls_command`.
- `web/transport/dev_stream.rs`.

Status: implemented for command wiring and generated-output proof. IPTV and HLS now resolve active playout graphics from the DB,
render runtime/media/EPG/per-item variables into template-bearing config
strings, write generated VGS/ASS artifacts under per-stream temp graphics
folders, and pass z-ordered native/drawvg `GraphicsPipelineOverlay` values into
the shared FFmpeg pipeline. The dev MPEG-TS builder now accepts the same overlay
plan so generated graphics can be exercised through that path. Focused tests
cover per-element variable rendering into generated artifacts plus HLS/dev command
attachment. IPTV MPEG-TS, HLS segmenter, and dev MPEG-TS drawvg runtime proofs
now generate real FFmpeg output and verify the composited pixel path. IPTV/HLS
temp graphics directories are bounded to per-request/per-item folders and
removed after the FFmpeg child exits or after render failure. A live scheduled
graphics proof now imports ErsatzTV-style YAML elements, attaches
Image/Text/Subtitle/Motion through `PlayoutItemGraphicsElement`, resolves and
renders them through playback, encodes the shared FFmpeg pipeline, and decodes
the expected colored pixels from the final MPEG-TS stream.

Push `drawvg` filter strings into `request.drawvg_filters` and add any image/motion overlay
inputs; the pipeline already routes `graphics_overlay` (`pipeline/mod.rs:714-720`).

## Phase 6 — Schedule-integration completeness

- [x] **Classic:** `ProgramScheduleItemGraphicsElement` IDs are mirrored into
  playout-item graphics join rows during item construction/persistence.
- [x] **Block:** `BlockItemGraphicsElement` IDs flow through the common playout-item
  helper and persist as `PlayoutItemGraphicsElement` rows; deco graphics remain
  selected by the Phase 2 resolver.
- [x] **Scripted:** `graphics_on`/`graphics_off`(`_path`) persist active graphics
  rows, and `graphics_on_path(path, variables_json)` now carries the variables
  string into the join `Variables` column.
- [x] **Sequential/YAML:** `graphics_on` / `graphics_off` maintain active graphics
  rows and serialize YAML `variables:` into per-item `Variables`.
- [x] Confirmed the playout build repository writes and reads generated
  `PlayoutItemGraphicsElement` rows with per-item `Variables`.

## Phase 7 — Assets & templates

- [x] Add `assets/graphics/` templates for the in-scope generated-artifact formats:
  `lower-third.vgs` and `song-progress.vgs` source templates for FFmpeg `drawvg`,
  plus `subtitle-template.ass` for libass subtitle graphics.
- [x] Document template variables in `assets/graphics/README.md`, including that
  `lower-third.svg` is a source/reference fixture and not direct drawvg input.
- [x] Add focused template coverage in
  `graphics_assets_render_drawvg_and_ass_templates`, including a guard that checked-in
  drawvg assets render to VGS commands instead of SVG/XML.

## Phase 8 — Tests & verification

- **Snapshot tests** (`crates/ersatzrs-ffmpeg/tests/pipeline_snapshots.rs`, follow
  `filter_graph_with_watermark_subtitle_and_drawvg_snapshot`): one per kind (image overlay,
  text drawvg, subtitle `.ass`, motion video overlay) plus a multi-element z-order case.
- **Unit tests:** element resolution/precedence, variable-merge precedence, template
  substitution (incl. Motion `start_seconds` arithmetic and EPG fields), enable windows.
- **End-to-end:** `cargo run --bin ersatzrs-web`, attach a text + image element to a
  channel's schedule, view the stream, confirm both composite in z-order. Note
  `LD_LIBRARY_PATH` for local ffmpeg-8.1.

## Phase 9 — Docs

- [x] Update ADR 0005 to record the per-kind rendering split, DB-canonical
  graphics storage with YAML upsert import, VGS drawvg artifacts, and retirement
  of the `0002` geometry/text stopgap by migration `0003`.
- [x] Update progress/parity sources of truth for the graphics-engine row and
  session log. `docs/reference/parity-matrix.md` and `STATUS.md` are moved stubs
  that point to `docs/progress`; no hand-edited table remains there.
- [x] Run the broad gates: fmt, default workspace clippy, mysql-feature
  workspace clippy, and workspace all-features tests.
- [x] Finish the completion audit and live end-to-end evidence before marking
  the graphics-engine goal complete.

---

## Critical files

- `crates/ersatzrs-infra/src/playback.rs` — add `select_playback_graphics` (mirror
  `select_playback_subtitle`).
- `crates/ersatzrs-core/src/repositories/graphics.rs`,
  `crates/ersatzrs-infra/src/repositories/graphics.rs` — `map_element`/`upsert_element` for the
  new field set; resolve from all join tables; carry `Variables`.
- `migrations/0003_*.sql` (+ `migrations/mysql/0003_*.sql`) — replace the `0002` geometry
  columns with the full `GraphicsElement` field set (Decision C).
- **New** graphics-element YAML importer (e.g. `crates/ersatzrs-infra/src/graphics_import.rs`)
  — parse ErsatzTV YAML → `upsert_element`, using `serde_yaml`.
- `crates/ersatzrs-ffmpeg/src/graphics/mod.rs` — per-kind drawvg/native rendering + render-time
  template variable substitution.
- `crates/ersatzrs-ffmpeg/src/pipeline/mod.rs:714-720` — graphics overlay wiring (already
  present; extend for image/motion overlay inputs + z-order).
- `crates/ersatzrs-web/src/transport/iptv/stream.rs`,
  `crates/ersatzrs-infra/src/hls_worker.rs`,
  `crates/ersatzrs-web/src/transport/dev_stream.rs` — streaming builders.
- `crates/ersatzrs-core/src/scheduling/scripted.rs` — `graphics_on/off(_path)` hooks.
- `crates/ersatzrs-domain/src/graphics.rs` — `GraphicsElement` struct reshaped to the new
  field set (drop the `x/y/width/height`/`script`/`variables` phantoms).
- `assets/graphics/` — VGS/`.ass` templates plus source SVG fixtures.
- `crates/ersatzrs-ffmpeg/tests/pipeline_snapshots.rs` (+ `tests/snapshots/`).

## Reused, do not reinvent

- `GraphicsRepository`, `GraphicsTemplateRenderer`, `SvgTemplate`, `DrawvgOverlay`,
  `lower_third_overlay`, and the `request.drawvg_filters` → `graphics_overlay` pipeline wiring.
- The subtitle filter path (`subtitles`/`ass`) for Subtitle elements.
- Watermark overlay/scale helpers (`filter/simple.rs`) for Image/Motion overlays — land via
  the watermark plan first.
- Scripted-schedule `graphics_on/off(_path)` plumbing.

## Domain-model gap (investigated) & resolved decision (C)

**Finding — how ErsatzTV stores an element vs. how ErsatzRS does:**

- **ErsatzTV:** the `GraphicsElement` DB row holds **only** `Id`, `Path`, `Name`, `Kind`
  (`$ERSATZTV_SRC/ErsatzTV.Core/Domain/GraphicsElement.cs`; the EF config maps nothing else).
  Every rich field in the table above lives in a **YAML file at `Path`**, Scriban-preprocessed
  and deserialized at render time by `GraphicsElementLoader` into the
  `ErsatzTV.Core/Graphics/*GraphicsElement` classes. Per-item `Variables` (JSON) on
  `PlayoutItemGraphicsElement` are merged in.
- **ErsatzRS today:** `0001_initial.sql` matches ErsatzTV (`Path/Kind/Name`), **but**
  `0002_drawvg_graphics_engine.sql` adds ErsatzRS-specific columns
  `X, Y, Width, Height, ZIndex, EnableExpression, Text` (the `GraphicsElement` struct +
  repository read/write these). The struct also has `script`/`variables` fields that are
  **never persisted** (`map_element` hardcodes them to `None`; `upsert_element` omits them).
  There is **no editor UI** for element definitions — `graphics_element_ids` is only
  *attached* to schedule/block/deco items — so the model is half-built.
- **Net:** ErsatzRS replaced ErsatzTV's rich per-kind YAML config with a flat drawvg-oriented
  geometry row (`x/y/width/height/z_index/enable/text`). That row cannot express
  `opacity_percent`, `location`, margins, `scale_width_percent`, fonts/`styles`,
  `video_path`/`start_seconds`/`end_behavior`, etc. — so it is not a faithful port and cannot
  drive the four element kinds as documented.

**Resolved — Decision C (DB-primary + YAML import):** the DB is the canonical store for
graphics elements in ErsatzRS, **and** an ErsatzTV-style YAML file at `Path` can be parsed to
**upsert** a fully-populated `GraphicsElement` into the DB. This keeps ErsatzRS DB-native
(queryable, admin-editable, no per-frame file parsing) while preserving an ErsatzTV import path
for parity. Consequences:

- **Schema/domain extended to hold the full field set.** Replace the half-built `0002`
  geometry columns with the real model (migration `0003`): shared columns for the common fields
  (`Kind`, `Name`, `Path`, `ZIndex`, `OpacityPercent`, `OpacityExpression`, `Location`,
  `HorizontalMarginPercent`, `VerticalMarginPercent`, `LocationX`, `LocationY`, `Scale`,
  `ScaleWidthPercent`, `PlaceWithinSourceContent`, `EpgEntries`) plus a JSON `Config` column for
  kind-specific extras (Text: `styles` list + `text`/`text_fit`/`text_align`/`width_percent`/
  `base_style`/`include_fonts_from`; Motion: `video_path`/`start_seconds`/`hold_seconds`/
  `end_behavior`; Subtitle: `template`; Script: `command`/`args`/`format`/`pixel_format`/
  `duration_seconds`). `Path` remains the link back to the source YAML for re-sync.
- **YAML importer** parses the ErsatzTV YAML (honoring the `YamlMember` aliases in
  `ErsatzTV.Core/Graphics/*`) → domain `GraphicsElement` → `upsert_element`. Provide an import
  entry point (admin action and/or a templates-folder sync at startup).
- **Render-time templating stays render-time.** Text/template strings keep their `{{ }}`
  placeholders in the DB; runtime variables (`epg_entries`, `MediaItem_Duration`, time, Motion
  `start_seconds` arithmetic) are substituted/evaluated when the stream command is built — not
  baked in at import.
- **Admin editor** for DB-backed elements is desirable but secondary (DB is already the source
  of truth); the importer is the parity-critical piece.
- **`Script` kind:** ErsatzTV's `ScriptGraphicsElement` runs an **external `command`/`args`
  process** — exactly the external renderer ADR-0005 forbids. Import its fields for round-trip
  fidelity, but render via the existing `mlua` scripted engine emitting drawvg scripts, or defer
  the `Script` kind. Do **not** spawn a child graphics process.

## Verification checklist

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings` then a second pass with
   `--features mysql` (per project lint policy).
3. `cargo test --workspace --all-features` + `cargo insta review` for new snapshots.
4. Manual: run `ersatzrs-web`, attach graphics elements to a channel, confirm they composite
   in z-order in the live stream.
