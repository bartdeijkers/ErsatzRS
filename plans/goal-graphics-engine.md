# Goal prompt — Graphics engine to completion

Standing prompt to continue graphics-engine work until done. The detailed,
code-grounded plan is **`plans/graphics-engine-parity-slice.md`** (9 phases) —
read it first; it is authoritative. This is the resumable brief on top.

## Goal

Port ErsatzTV's graphics engine so image/text/subtitle/motion elements render in
the final video for every streaming path (IPTV `.ts`/MPEG-TS, HLS, dev stream),
attached through every schedule type — faithfully, **no stopgaps**, in the
drawvg architecture (no external graphics process).

## Locked decisions (do not relitigate)

- **ADR 0005:** engine is FFmpeg 8.1 `drawvg` + VGS templates in `assets/`.
- **Storage = Decision C:** DB is canonical; an ErsatzTV-style YAML file at
  `GraphicsElement.Path` is parsed to **upsert** a fully-populated element.
  Kind-specific extras live in a JSON `Config` column; shared fields are typed.
- **Rendering split:** vector/text kinds (`Text`, `Subtitle`, `Script`) render
  via `drawvg` (libass for `.ass` subtitles); raster/video kinds (`Image`,
  `Motion`) via native FFmpeg `overlay`. Templating stays render-time.
- **`Script` kind:** import fields for fidelity, never spawn a process — render
  via the `mlua` engine emitting drawvg scripts, or defer it.

## Progress so far

- **Migration `0003_graphics_element_fields.sql`** (SQLite) + `migrations/mysql/
  0003_*` drafted: drop `0002` geometry stopgaps, keep `ZIndex`, add shared
  typed columns + JSON `Config` column.
- **`crates/ersatzrs-domain/src/graphics.rs`** reshaped: `GraphicsElement` has
  shared typed fields + `config: GraphicsElementConfig` (internally-tagged enum;
  `StyleDefinition`, `TextFit`, `TextAlignment`, `MotionEndBehavior` ported).
  Phantom fields removed; `WatermarkGraphicsState` dropped `Eq`.
  `cargo check -p ersatzrs-domain` passes.
- **Data layer / importer (Phase 1)** implemented:
  `crates/ersatzrs-infra/src/repositories/graphics.rs` maps the shared columns,
  serializes/deserializes `Config` with `serde_json`, treats `NULL` as the
  row-kind default, and rejects malformed or kind-mismatched JSON.
  `crates/ersatzrs-infra/src/graphics_import.rs` parses ErsatzTV YAML with the
  aliases from `$ERSATZTV_SRC/ErsatzTV.Core/Graphics/*GraphicsElement.cs`,
  upserts through `GraphicsRepository`, and keeps `Path` as the source link.
  Focused `graphics_repository` / `deco_repository` tests pass.
- **Resolve & load (Phase 2)** implemented:
  `select_playback_graphics` in `crates/ersatzrs-infra/src/playback.rs`
  resolves active template-deco graphics, playout-deco graphics, and
  playout-item graphics with ErsatzTV `GraphicsElementSelector` merge /
  override / disable semantics, honors `UseGraphicsElementsDuringFiller`, loads
  per-item `Variables`, and returns concrete `GraphicsElement` rows sorted by
  `z_index`. Focused playback resolver tests pass.
- **Templating & variables (Phase 3)** implemented:
  `crates/ersatzrs-infra/src/graphics_template.rs` builds an ErsatzTV-compatible
  render-time variable map (`Resolution`, `ScaledResolution`,
  `Channel_StartTime`, `MediaItem_*`, `Epg`) from playback context + DB media /
  playout rows, merges per-item JSON variables last, renders `{{ ... }}`
  placeholders with dotted/indexed lookup, and evaluates numeric `+`/`-`
  expressions such as `MediaItem_Duration.TotalSeconds - 5` for Motion timing.
  Focused unit/integration tests pass.
- **Per-kind rendering (Phase 4, Image/Text slice)** started:
  `ersatzrs-ffmpeg::GraphicsTemplateRenderer::render_graphics_elements` now
  splits DB-backed graphics by kind: Image elements become looped native
  `graphics_image` inputs plus `WatermarkOverlay`-style scale/location/opacity
  metadata, while Text elements write deterministic full-frame VGS draw scripts and
  return `DrawvgOverlay` filters. `PipelineRequest::graphics_overlays` now
  interleaves native image overlays and drawvg filters by `z_index`, and
  `FFmpegPipeline` carries finalized generated inputs so the emitted command is
  valid. Focused render-plan and mixed native/drawvg command snapshots pass.
  Motion native overlay rendering now covers looped inputs, start-time PTS
  offset, scale/location/margins, overlay enable, and disappearing EOF behavior
  in the same z-ordered pipeline path. Subtitle graphics now render `.ass`
  templates to generated files, then use transparent lavfi/libass overlay
  inputs in the same graphics z-order chain. IPTV, HLS, and dev MPEG-TS command
  builders now accept the generated graphics overlay plan; IPTV/HLS resolve
  active playout graphics from the DB, render template-bearing config strings
  with runtime/media/EPG/per-item variables, write generated VGS/ASS artifacts,
  and pass the resulting native/drawvg overlays into the shared FFmpeg pipeline.
  IPTV graphics artifacts now use per-request temp folders that are removed
  after the FFmpeg stream child exits or on render failure, and the focused
  ignored MPEG-TS drawvg smoke proves real FFmpeg output after the post-drawvg
  `format=yuv420p` conversion.
  Motion Hold now probes source duration in infra and bounds native overlay
  visibility to `start_seconds + source_duration + hold_seconds`, with the
  upstream-style zero-duration fallback when probing fails. Script graphics now
  execute `command` as an in-process Lua script that returns a drawvg script,
  with `variables`, `args`, `frame`, and `element` globals available. Phase 6
  schedule integration now persists generated playout-item graphics join rows:
  classic/block schedule IDs are mirrored onto `PlayoutItemGraphicsElement`,
  and YAML/scripted `graphics_on` variables are retained in the join `Variables`
  column for the playback resolver.

## Work remaining (execute in order)

1. **Assets/templates (7)** — add and validate the durable graphics templates/assets
   now that IPTV MPEG-TS, HLS, and dev MPEG-TS drawvg output are runtime-proven.
2. **Broader tests (8)**, **docs + ADR for rendering
   split / `0002` retirement (9)**.

## Working rules

- Read `$ERSATZTV_SRC` before porting each piece. Preserve patterns, YAML
  aliases, JSON shapes, EF-style `PK_/FK_/IX_` names.
- Respect the configured cargo target-dir; no other path.
- Keep `STATUS.md`, `docs/reference/parity-matrix.md`, and the health check
  current.

## Definition of done

- Image + text (+ subtitle/motion) elements on a channel's schedule composite
  in z-order in the live stream; an ErsatzTV YAML element imports and renders.
- `cargo fmt --all -- --check` clean.
- `cargo clippy --workspace --all-targets -- -D warnings`, then `--features
  mysql`.
- `cargo test --workspace --all-features` green; snapshots accepted.
- ADR added/updated; parity matrix + STATUS updated; on a branch.
