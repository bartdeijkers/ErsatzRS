---
name: ffmpeg-snapshot-test
description: Add an insta snapshot test for generated FFmpeg argv, filter graphs, or drawvg fragments. Use whenever generated FFmpeg output changes.
---

# FFmpeg Snapshot Test

## Workflow

1. Read [docs/how-to/snapshot-tests.md](../docs/how-to/snapshot-tests.md).
2. Identify the generator being tested.
3. Build the smallest deterministic input.
4. Serialize output as stable text.
5. Use `insta::assert_snapshot!`.
6. Run `./run.sh test -p <crate> <snapshot-test-filter>` on POSIX or
   `.\run.cmd test -p <crate> <snapshot-test-filter>` on Windows.
7. Review with `cargo insta review`.
8. Rerun the same focused nextest command and confirm no `.snap.new` remains.

## Rules

- Never commit `.snap.new`.
- Snapshot command arguments in their final argv order.
- Keep the focused loop in one Cargo feature/target configuration; run broad
  playback and closeout gates only after the snapshot is accepted.
- Add an ADR link in the test or PR if the snapshot encodes a deliberate
  ErsatzTV deviation.
