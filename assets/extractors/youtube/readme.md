# YouTube remote-stream extractor for Windows

`youtube.bat` resolves a YouTube video with `yt-dlp`, seeks the remote media
source with ErsatzRS's managed FFmpeg runtime, and writes MPEG-TS to standard
output. It is intended for finite videos in an ErsatzRS Remote Streams library.

`yt-dlp` remains an optional manual download. ErsatzRS does not install, update,
or bundle it. The wrapper searches for `yt-dlp.exe` in this order:

1. the path in `YT_DLP_BIN`;
2. beside `youtube.bat`;
3. at the release root when the wrapper remains under
   `assets\extractors\youtube`;
4. on `PATH`.

The wrapper requires `FFMPEG_BIN`. ErsatzRS sets it to the managed FFmpeg 9
runtime whenever it starts a Remote Stream producer. Requiring this variable
prevents an unrelated host FFmpeg installation from silently changing
playback behavior.

## Add a video to ErsatzRS

Create one YAML definition per video. The checked-in
`youtube-windows.yml.example` is the canonical template: keep `{seek}` as its
own quoted argument, mark the finite video as non-live, and record its exact
duration so scheduling remains deterministic.

```yaml
# Quote the complete script value at YAML level and each command argument.
script: '"C:\path\to\assets\extractors\youtube\youtube.bat" "https://www.youtube.com/watch?v=<video-id>" "{seek}"'
is_live: false
duration: "00:20:00"
title: "Example YouTube video"
```

ErsatzRS replaces `{seek}` with the current playout offset. The wrapper asks
`yt-dlp` for a combined H.264/AAC rendition and makes it launch `FFMPEG_BIN` as
the external downloader. yt-dlp supplies the extractor-specific request
headers while `ffmpeg_i` puts `-ss` before FFmpeg's remote input. The managed
FFmpeg process stream-copies the selected video/audio pair to MPEG-TS. This
avoids making the channel consumer discard the beginning of a non-seekable
producer pipe when a viewer tunes in partway through a video.

Do not resolve a temporary YouTube media URL in one process and open it with a
separate FFmpeg command: those URLs can require the request headers selected by
the extractor. Keeping yt-dlp in charge of the external downloader preserves
that request context while still guaranteeing the managed FFmpeg executable.

Do not add `url` to the same definition because `url` takes precedence over
`script`. Keep diagnostics on standard error; standard output is reserved for
media bytes. Successful extraction is deliberately quiet so temporary signed
media URLs do not leak into a console or captured log. Failures still return a
non-zero exit code with a concise diagnostic on standard error.

## Test the wrapper manually

Set `FFMPEG_BIN` to the managed Windows executable, then capture a short sample:

```batch
set "FFMPEG_BIN=C:\path\to\managed\ffmpeg.exe"
youtube.bat "https://www.youtube.com/watch?v=<video-id>" "00:10:00" > sample.ts
```

Inspect `sample.ts` with `ffprobe` and confirm that it contains both video and
audio. Access and redistribute material only when authorized, and comply with
the service terms and applicable law.
