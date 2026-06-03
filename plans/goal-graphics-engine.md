# Goal prompt — Graphics engine to completion

Use this as the standing prompt to continue the graphics-engine work until it is fully done.
Paste it to an agent (or follow it yourself) and iterate until the Definition of Done is met.

---

## Goal

Port ErsatzTV's graphics engine into ErsatzRS so image/text/subtitle/motion graphics
elements render in the final video output for every streaming path (IPTV `.ts`/MPEG-TS,
HLS, dev stream), attached through every schedule type — faithfully, **no stopgaps**, and
within the drawvg architecture (no external graphics process).

The detailed, code-grounded plan is **`plans/graphics-engine-parity-slice.md`** (9 phases) —
read it first and keep it authoritative. This file is the resumable brief on top of it.

## Locked decisions (do not relitigate)

- **ADR 0005:** the engine is FFmpeg 8.1 `drawvg` + SVG templates in `assets/`. **Do not**
  reintroduce an external graphics process.
- **Storage = Decision C:** the DB is the canonical store; an ErsatzTV-style YAML file at
  `GraphicsElement.Path` is parsed to **upsert** a fully-populated element. Kind-specific
  extras live in a **JSON `Config` column**; shared fields are typed columns.
- **Rendering split:** vector/text kinds (`Text`, `Subtitle`, `Script`) render via `drawvg`
  (libass for `.ass` subtitles); raster/video kinds (`Image`, `Motion`) render via native
  FFmpeg `overlay`. Render-time templating stays render-time (DB keeps `{{ }}` placeholders).
- **`Script` kind:** import its fields for fidelity, but never spawn an external process —
  render via the existing `mlua` scripted engine emitting drawvg SVG, or defer the kind.

## Progress so far (already done)

- **Migration `0003_graphics_element_fields.sql`** (SQLite) and `migrations/mysql/0003_*`
  drafted: drop the `0002` geometry stopgaps, keep `ZIndex`, add the shared typed columns +
  the JSON `Config` column.
- **`crates/ersatzrs-domain/src/graphics.rs`** reshaped: `GraphicsElement` now has shared
  typed fields + `config: GraphicsElementConfig` (internally-tagged enum over per-kind structs;
  `StyleDefinition`, `TextFit`, `TextAlignment`, `MotionEndBehavior` ported). Phantom
  `x/y/width/height/enable_expression/text/script/variables` removed. `WatermarkGraphicsState`
  dropped `Eq` (floats). `cargo check -p ersatzrs-domain` passes.

## Work remaining (resume here — execute in order)

1. **Finish the data layer (Phase 1).** The reshape currently breaks the infra layer:
   - Rewrite `map_element` / `upsert_element` in
     `crates/ersatzrs-infra/src/repositories/graphics.rs` for the new columns + `serde_json`
     (de)serialization of the `Config` JSON (NULL ⇒ kind default).
   - Fix `crates/ersatzrs-infra/tests/graphics_repository.rs` and `deco_repository.rs`.
   - Build the **YAML importer** (e.g. `crates/ersatzrs-infra/src/graphics_import.rs`): parse
     ErsatzTV YAML (honor the `YamlMember` aliases in `$ERSATZTV_SRC/ErsatzTV.Core/Graphics/*`)
     → `upsert_element`, keeping `Path` as the source link. Expose an import entry point.
2. **Resolve & load for streaming (Phase 2)** — `select_playback_graphics` (deco → playout-deco
   → playout-item; `Deco.GraphicsElementsMode` + `UseGraphicsElementsDuringFiller`); mirror
   `select_playback_subtitle`. Port `GraphicsElementSelector` precedence.
3. **Render-time templating & variables (Phase 3)** — epg entries, `MediaItem_Duration`, time,
   Motion `start_seconds` arithmetic; merge element vs per-item `Variables`.
4. **Per-kind rendering (Phase 4)** — Image→overlay, Text→drawvg SVG, Subtitle→libass,
   Motion→looped/seeked video overlay, Script→mlua→drawvg; composite by `z_index`.
5. **Wire into streaming (Phase 5)** — push `drawvg_filters` + overlay inputs in the three
   builders (the pipeline already routes `graphics_overlay`).
6. **Schedule integration (Phase 6)**, **assets/templates (Phase 7)**, **tests (Phase 8)**,
   **docs incl. ADR for the rendering split / `0002` retirement (Phase 9)**.

## Working rules

- Read the ErsatzTV source under `$ERSATZTV_SRC` before porting each piece. Preserve patterns,
  YAML field aliases, JSON shapes, and EF-style `PK_/FK_/IX_` identifier names.
- Respect the configured cargo target-dir; never use a different build path.
- Keep `STATUS.md`, `docs/reference/parity-matrix.md`, and the health-check engine current.

## Definition of done

- Image + text (and subtitle/motion) graphics elements attached to a channel's schedule
  composite in z-order in the live stream; an ErsatzTV YAML element imports cleanly and renders.
- `cargo fmt --all -- --check` clean.
- `cargo clippy --workspace --all-targets -- -D warnings`, then a second pass with
  `--features mysql`, both clean.
- `cargo test --workspace --all-features` green; new snapshots reviewed/accepted.
- ADR added/updated (rendering split + retiring the `0002` columns); parity matrix + STATUS
  updated; changes committed on a branch.
