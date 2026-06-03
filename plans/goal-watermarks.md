# Goal prompt — Watermarks to completion

Use this as the standing prompt to continue the watermark work until it is fully done.
Paste it to an agent (or follow it yourself) and iterate until the Definition of Done is met.

---

## Goal

Make channel watermarks render in the final video output of ErsatzRS for every streaming
path (IPTV `.ts`/MPEG-TS, HLS, dev stream) and every watermark mode (Permanent,
Intermittent, OpacityExpression), faithfully porting ErsatzTV — **no stopgaps**. The
reported bug is "channel logo watermark not shown on TV" (the Permanent case).

The detailed, code-grounded plan is **`plans/watermark-parity-slice.md`** — read it first
and keep it authoritative. This file is the resumable brief on top of it.

## Locked decisions (do not relitigate)

- Watermarks render via **native FFmpeg `overlay` + `fade`/alpha-expression filters**; no
  external Skia/graphics process (ADR 0005 / AGENTS.md).
- `drawvg` is reserved for the separate graphics-engine track (`plans/goal-graphics-engine.md`);
  do not route watermarks through it. `Resource`-sourced watermarks (song progress) belong to
  the graphics-engine track.

## Root cause (already diagnosed)

The FFmpeg watermark infrastructure is fully built and snapshot-tested but **never wired in**:
nothing sets `request.watermark`. `Channel.watermark_id`, `ffmpeg.global_watermark_id`, and
`PlayoutItem.watermark_ids` are populated by the scheduler but consumed nowhere downstream.
The subtitle path (`select_playback_subtitle` → `Option<InputFile>` → `build_stream_command`)
is the proven template to mirror.

## Work remaining (from the plan — execute in order)

1. **Resolution & loading** — port `WatermarkSelector` precedence (Deco → Playout →
   PlayoutItem → Channel → Global, honoring `disable_watermarks`); add
   `select_playback_watermarks` in `crates/ersatzrs-infra/src/playback.rs` and a watermark
   loader (mirror the `graphics.rs` repository).
2. **Image-source resolution** — Custom (`data/watermarks/<image>`), ChannelLogo (`Artwork`
   `ArtworkKind = 2`). Resource → defer to graphics engine.
3. **FFmpeg overlay parity** — fix the two real bugs: add `width_percent` + a
   `watermark_scale_filter` (`scale={round(width%·frame_w)}:-1`), and the still-image `-loop 1`
   input option on `InputFile::watermark`. Map all modes (Permanent/Intermittent via fade
   points/OpacityExpression via alpha expression); support multiple watermarks in `z_index`
   order in `pipeline/mod.rs:653-673`.
4. **Wire into streaming** — set `request.watermark` + push the watermark input in
   `transport/iptv/stream.rs`, `infra/hls_worker.rs`, and `web/transport/dev_stream.rs`.
5. **Tests** — `insta` snapshots (permanent+scaled, channel-logo, intermittent, opacity-
   expression, multiple) and unit tests (selection, image resolution, fade-point generation,
   opacity-expression translation, loop option).

## Working rules

- Read the ErsatzTV source under `$ERSATZTV_SRC` before porting each piece (files are listed
  in the plan). Preserve patterns, JSON shapes, and EF-style identifier names.
- Respect the configured cargo target-dir; never use a different build path.
- Keep `STATUS.md`, `docs/reference/parity-matrix.md`, and the health-check engine current.

## Definition of done

- A channel with a Permanent channel-logo watermark shows the logo composited at the
  configured location/opacity/scale on a real player/TV.
- Intermittent and OpacityExpression modes work; multiple watermarks composite in z-order.
- `cargo fmt --all -- --check` clean.
- `cargo clippy --workspace --all-targets -- -D warnings`, then a second pass with
  `--features mysql`, both clean.
- `cargo test --workspace --all-features` green; new snapshots reviewed/accepted.
- Docs updated (parity matrix row, STATUS session log) and changes committed on a branch.
