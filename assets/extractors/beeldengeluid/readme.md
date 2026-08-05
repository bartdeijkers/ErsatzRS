# Beeld & Geluid remote-stream extractor

These standalone wrappers stream an episode from the Beeld & Geluid
[Schatkamer](https://schatkamer.beeldengeluid.nl/) to standard output as
MPEG-TS. They are intended for an ErsatzRS Remote Streams library and implement
the `script` behavior described by the
[ErsatzTV Remote Stream documentation](https://ersatztv.org/docs/media/local/remotestreams/definition/).

Each wrapper implements the complete site-specific flow itself:

1. Extract the episode ID from the Schatkamer URL.
2. Discover the current `getProgramStreamById` Server Action from the episode
   page's JavaScript chunks.
3. Request fresh stream data while preserving the page session cookies.
4. Parse the exact-length React Server Component response chunks.
5. Move the signed CloudFront parameters from the HLS query string into
   request cookies.
6. Remux every returned HLS asset's first video/audio rendition pair to
   MPEG-TS on standard output.

No yt-dlp installation, Python runtime, browser cookies, or manually copied
`.m3u8` URL is required.

The YAML examples require an ErsatzRS build that executes the Remote Stream
`script` field and forwards its standard output. The wrapper itself does not
install or enable that server-side playback capability.

## Requirements

Both wrappers require FFmpeg 9 and network access from the ErsatzRS process to:

- `schatkamer.beeldengeluid.nl`
- `sk-video.cdn.beeldengeluid.nl`

The Linux/macOS wrapper also uses standard command-line tools normally present
on those systems: `curl`, `grep`, `dd`, `sed`, and `mktemp`.

The Windows wrapper uses `curl.exe` and Windows PowerShell, which are included
with supported Windows versions. Set these optional environment variables when
an executable is not on `PATH`:

| Variable | Default | Purpose |
| --- | --- | --- |
| `FFMPEG_BIN` | `ffmpeg` or `ffmpeg.exe` | FFmpeg executable path |
| `CURL_BIN` | `curl` or `curl.exe` | curl executable path |
| `BEELDENGELUID_ACTION_ID` | discovered automatically | Emergency override for the hexadecimal Server Action ID |

On Linux or macOS, make the wrapper executable:

```sh
chmod +x beeldengeluid.sh
```

## Test the wrapper

Use a Schatkamer episode URL with the following structure:

```text
https://schatkamer.beeldengeluid.nl/serie/<series-id>/<series-slug>/aflevering/<episode-id>
```

On Linux or macOS:

```sh
./beeldengeluid.sh "<episode-url>" > episode.ts
```

On Windows:

```batch
beeldengeluid.bat "<episode-url>" > episode.ts
```

Open `episode.ts` with a media player or inspect it with `ffprobe`. Diagnostics
and failures are written to standard error; standard output contains only
media bytes. Selecting one rendition pair keeps the output stream order stable
as video then audio, which is the layout ErsatzRS channel playback expects.
The wrapper remuxes with stream copy and does not normalize framerate itself;
any later transcoding uses ErsatzRS's FFmpeg 9 playback pipeline.

## Add the episode to ErsatzRS

1. Create a directory for the Remote Streams library.
2. Add one `.yml` definition per episode.
3. In ErsatzRS, open **Media Sources** -> **Local** and create a library with
   media kind **Remote Streams** whose path is that directory.
4. Open **Media** -> **Libraries** and scan the new library.
5. Add the imported remote-stream item to a collection or schedule it on a
   channel.

Linux or macOS definition:

```yaml
script: '"/path/to/beeldengeluid.sh" "https://schatkamer.beeldengeluid.nl/serie/<series-id>/<series-slug>/aflevering/<episode-id>"'
is_live: false
title: "Episode title"
```

Windows definition:

```yaml
script: '"C:\path\to\beeldengeluid.bat" "https://schatkamer.beeldengeluid.nl/serie/<series-id>/<series-slug>/aflevering/<episode-id>"'
is_live: false
title: "Episode title"
```

Schatkamer episodes are video on demand, so `is_live` must be `false`.
`duration` is optional for VOD. If supplied, use `HH:MM:SS`.

The command path and the required executables must be visible to the ErsatzRS
process itself. For a container deployment, extend the image with `curl` and
make the managed FFmpeg executable available to the wrapper through
`FFMPEG_BIN`. Host-installed tools are not automatically available inside the
container.

## Operational notes

- Keep the complete episode URL quoted because it may contain shell-sensitive
  characters.
- Do not add `url` to the same YAML file. Under the Remote Stream contract,
  `url` takes precedence over `script`.
- Do not redirect diagnostics to standard output; ErsatzRS treats all standard
  output as MPEG-TS content.
- Signed CloudFront data is short-lived. The wrapper requests fresh stream
  data every time it starts.
- Access and redistribute material only when you are authorized to do so and
  comply with the service terms and applicable law.
