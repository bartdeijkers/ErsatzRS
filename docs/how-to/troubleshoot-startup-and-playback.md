# Troubleshoot startup and playback

Use this guide when the admin UI does not open, a library will not scan, or a
channel will not play.

## Start with readiness

From the ErsatzRS host, request:

```bash
curl --verbose http://127.0.0.1:8409/healthz
```

- `200 OK` means database startup and the FFmpeg runtime are ready.
- `503 Service Unavailable` means startup completed far enough to serve HTTP,
  but a readiness requirement failed.
- Connection refused usually means the process stopped, uses another address,
  or could not bind port `8409`.

For Docker, inspect container state and logs:

```bash
docker ps --all --filter name=ersatzrs
docker logs ersatzrs
```

## Use the support pages

When the admin UI opens, use these pages in order:

1. **Support** -> **Health Checks** for database, schema, FFmpeg, media,
   schedule, provider, and runtime readiness.
2. **Support** -> **Logging** for server and background-task messages.
3. **Support** -> **Troubleshooting** to inspect a media item, schedule,
   playout, channel, or in-app TSDuck MPEG-TS result.

Copy only redacted diagnostics. Do not publish provider tokens, database URLs
with passwords, private media paths, or playlist URLs containing access tokens.

## Resolve common startup failures

### Port 8409 is already in use

Stop the conflicting process or set another listen address:

```text
LEPTOS_SITE_ADDR=0.0.0.0:8410
```

Update the Docker port publication and IPTV client URLs to match.

### FFmpeg download or validation fails

ErsatzRS requires FFmpeg and ffprobe 8.1 or newer with required filters such as
`drawvg` and `zscale`. Normally it downloads a matching managed runtime.

Check internet access, release-asset availability, filesystem permissions, and
free disk space. If you supply your own runtime, set both FFmpeg and ffprobe
path overrides and verify the binaries belong to the same build.

See [Why ErsatzRS manages FFmpeg](../explanation/managed-ffmpeg-runtime.md) and
the [configuration reference](../reference/configuration.md).

### Database or migration health fails

Do not delete or edit migration records to force startup. Back up the state
directory, then inspect the exact schema health result. Unknown migrations can
mean the database came from a newer build; checksum mismatches can mean the
binary and database migration history do not match.

## Resolve library scan failures

1. Confirm the path is visible from the ErsatzRS process.
2. For Docker, run `docker exec ersatzrs find /media -maxdepth 3 -type f`.
3. Confirm the container or service account can read the files and directories.
4. Open the library row's **Search Media** action after the scan.
5. Troubleshoot an imported item to inspect its resolved path and ffprobe data.

## Resolve a channel that does not play

Check the chain from source to output:

1. The media item is available and has a readable media version.
2. The collection, playlist, search, or other schedule source returns items.
3. The schedule has saved items.
4. The channel has a playout.
5. **Playout Detail** contains active or upcoming items.
6. The channel is enabled and has an FFmpeg profile.
7. The selected hardware acceleration is available, or the profile uses
   software encoding.

Then test both outputs directly:

```text
http://127.0.0.1:8409/iptv/channel/1.m3u8
http://127.0.0.1:8409/iptv/channel/1.ts
```

If software playback works but hardware playback fails, switch the channel to a
software FFmpeg profile, restart the stream, and inspect health results for the
hardware probe.

## Resolve a rendered but unresponsive admin UI

If pages render but buttons and dialogs do nothing, the browser hydration bundle
may not match the server binary.

- Use an official archive without replacing its `site/` or `public/` folders.
- Recreate a Docker container from one image tag instead of mixing assets.
- Hard-refresh the browser after updating.
- Check browser requests under `/pkg/` for missing JavaScript or WASM files.

See [Leptos runtime configuration](../explanation/leptos-runtime-config.md) for
the underlying reason.
