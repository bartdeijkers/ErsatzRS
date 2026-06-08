# HLS/MPEG-TS Review Fixes

## Scope

Address the two review findings against commit `a081685` without widening into
the concurrent colorspace-normalization work.

- P1: protect the internal FFmpeg streaming endpoints with the same IPTV
  JWT/access-token enforcement as public `/iptv/*` routes.
- P2: make `PlayoutUpdated` force the HLS worker's next process request back to
  the live wall-clock timeline instead of continuing from a stale buffered-ahead
  timestamp.

## Source Map

- Runtime routes:
  - `crates/ersatzrs-web/src/host/router.rs`
  - `crates/ersatzrs-web/src/transport/iptv/access_token.rs`
  - `crates/ersatzrs-web/src/transport/iptv/stream.rs`
- HLS worker state:
  - `crates/ersatzrs-infra/src/hls_worker.rs`
  - `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/HlsSessionWorker.cs`
- Progress docs:
  - `docs/progress/parity/hls-variant-playlists-segmenter.toml`
  - `docs/progress/parity/mpeg-ts-channel-streaming.toml`
  - `docs/progress/sessions.ndjson`

## P1 Plan - Protect Internal FFmpeg Routes

### Problem

`build_iptv_routes` applies `require_iptv_access` to `iptv_routes` and then
merges `internal_ffmpeg_routes` afterward. As a result,
`/ffmpeg/stream/{channel_number}` and `/ffmpeg/concat/{channel_number}` are not
covered when IPTV JWT enforcement is enabled. `/ffmpeg/stream/{channel_number}`
can stream MPEG-TS directly, so it must not be publicly reachable without the
same token contract.

### Implementation

1. Move the IPTV access-token middleware so it wraps the combined IPTV and
   internal FFmpeg route tree.
2. Preserve token propagation from wrapper/concat commands:
   - modern wrapper already reads `/iptv/channel/{n}.m3u8?mode=segmenter`.
   - legacy concat reads `/ffmpeg/concat/{n}?mode=ts-legacy`.
   - internal stream reads `/ffmpeg/stream/{n}?mode=ts-legacy`.
3. Ensure the internal concat and stream URLs include the existing
   `access_token` value when the external `.ts` request had one.
4. Keep `/iptv/logos` exempt only through the existing access middleware rule;
   do not add a broad `/ffmpeg` exemption.

### Tests

Add focused route/middleware tests proving:

- auth disabled: `/ffmpeg/concat/{n}` and `/ffmpeg/stream/{n}` remain reachable
  to the router path they are expected to hit.
- auth enabled with no token: both internal endpoints return unauthorized.
- auth enabled with valid `?access_token=`: internal endpoints pass middleware.
- `build_legacy_concat_url` and the concat playlist propagate token suffixes so
  FFmpeg can follow internal URLs under auth.

## P2 Plan - Treat PlayoutUpdated As Seek

### Problem

`playout_updated()` stores `HlsSessionState::PlayoutUpdated`, but
`hls_transcode_decision` only treats `SeekAndWorkAhead` as the state that uses
`wall_clock_now` for the next request timestamp. If a worker is buffered ahead
when playout is rebuilt, the next iteration can still select/transcode from
`transcoded_until`, which may point into the stale pre-rebuild timeline.

### Implementation

1. Update `hls_transcode_decision` so the pre-transition state
   `PlayoutUpdated` is treated as a seek decision and uses `wall_clock_now`.
2. Preserve the existing post-transition mapping:
   - `PlayoutUpdated` still advances through `next_hls_session_state` as the
     ErsatzTV table requires.
   - realtime throttling/work-ahead acceleration should not erase the fact that
     the request timestamp must be wall-clock based.
3. Confirm `start_at_zero` remains false for `PlayoutUpdated` unless upstream
   source proves otherwise; the point is to seek to the current live timeline,
   not to restart the current item from zero.
4. Keep the change isolated to decision calculation unless runtime evidence
   shows `_transcodedUntil` itself must also be reset immediately on
   notification.

### Tests

Add focused HLS worker tests proving:

- `hls_transcode_decision(PlayoutUpdated, transcoded_until_far_ahead, now, ...)`
  uses `now` as `request_time`.
- the same test covers both realtime and work-ahead branches.
- the existing transition table still maps `PlayoutUpdated` to
  `SeekAndWorkAhead`.

## Verification

Run the focused gates first:

```bash
cargo fmt --all -- --check
cargo test -p ersatzrs-web --features ssr iptv
cargo test -p ersatzrs-infra hls_worker::tests::
cargo clippy -p ersatzrs-web --all-targets -- -D warnings
cargo clippy -p ersatzrs-infra --all-targets -- -D warnings
cargo xtask progress check
git diff --check
```

If either fix changes runtime routing or worker behavior beyond the focused
tests, rerun the channel 1 smoke checks from
`plans/goal-hls-segmenter-mpegts-wrapper-parity.md`:

- HLS Segmenter playlist sampling across at least 60 seconds.
- `mode=ts` bounded MPEG-TS capture with ffprobe.
- `mode=ts-legacy` bounded MPEG-TS capture with ffprobe.

## Commit Scope

Commit only the files for these two review fixes and their progress/session
docs. Do not stage or modify:

- `plans/colorspace-normalization-parity.md`
- colorspace snapshot updates
- `docs/progress/parity/ffmpeg-profile-editor.toml`
