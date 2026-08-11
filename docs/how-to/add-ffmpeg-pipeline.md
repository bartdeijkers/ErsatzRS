# Add an FFmpeg pipeline

Use this guide when adding a new software or hardware FFmpeg path.

## Before you begin

Set `$ERSATZTV_SRC`, identify the owning
`docs/progress/parity/<slug>.toml`, and read [PORTING.md](../../PORTING.md).
Read the matching ErsatzTV pipeline builder before deciding that a Rust-side
behavior should differ.

## Steps

1. Identify the ErsatzRS target module in `ersatzrs-ffmpeg`.
2. Preserve the command-generation structure unless an ADR records a deviation.
3. Add unit tests for builder invariants.
4. Add `insta` snapshots for generated argv and filter graphs.
5. Add an end-to-end manual verification note if the path requires hardware.
6. Update the owning `docs/progress/parity/<slug>.toml` file.

## Required snapshot coverage

Every pipeline must include snapshots for:

- A minimal single video input.
- HLS output.
- Watermark or graphics overlay when supported.
- Subtitle handling when supported.

## Hardware paths

Check the current [hardware-acceleration
reference](../reference/system-requirements.md#hardware-acceleration) instead of
copying its backend list here. Record the required FFmpeg capabilities and the
exact hardware evidence still needed for the new path.

## Verify the pipeline

Run the narrowest relevant builder or filter test first:

```bash
./run.sh test -p ersatzrs-ffmpeg <test-filter>
```

Review every generated argv or filter-graph change:

```bash
cargo insta review
```

Then validate the updated progress record:

```bash
cargo xtask progress check
```

The pipeline is ready for review when focused tests pass, no unreviewed
`.snap.new` file remains, the parity record states any hardware-only manual
evidence still required, and an ADR owns every intentional ErsatzTV deviation.
