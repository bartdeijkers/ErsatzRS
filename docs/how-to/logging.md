# Logging

Use `tracing` for all runtime diagnostics.

## Span Names

Use this pattern:

```text
<crate>::<module>::<operation>
```

Examples:

```text
ersatzrs_ffmpeg::pipeline::build
ersatzrs_core::scheduling::generate
ersatzrs_web::iptv::playlist
```

## Fields

Include stable identifiers when they apply:

- `channel_id`
- `playout_id`
- `media_id`
- `media_source_id`
- `ffmpeg_profile_id`
- `request_id`

## Levels

- HTTP request accepted: `info`.
- Scheduling decisions: `debug`.
- FFmpeg process start/stop: `info`.
- FFmpeg stderr lines: `debug` unless they indicate a failure.
- Recoverable user/config errors: `warn`.
- Process crashes and data corruption risks: `error`.

## Secrets

Never log API keys, bearer tokens, cookies, database passwords, full connection
strings, or private config dumps.

Library code must not use `println!`.
