# Why copied portrait video loses its orientation in MPEG-TS

Most phone recordings store orientation as a display matrix in the container
rather than rotating the encoded pixels. A player reads that matrix and turns
the decoded frame for display.

MPEG-TS has no equivalent field. When FFmpeg copies the encoded stream into
MPEG-TS, the video bytes remain unchanged but the display instruction has no
place to go.

## Measured container behavior

A 90-degree-tagged source was copied with the supported FFmpeg 9 runtime:

| Output | Rotation after stream copy |
|---|---|
| MP4 | preserved |
| Matroska | preserved |
| HLS with fMP4 segments | preserved |
| MPEG-TS | lost |
| HLS with MPEG-TS segments | lost |

The loss can be reproduced with any file for which ffprobe reports rotation:

```bash
ffmpeg -y -i rotated.mp4 -c copy -f mpegts out.ts
ffprobe -select_streams v:0 -show_entries stream_side_data=rotation \
  -of csv=p=0 out.ts
```

The final command prints no rotation value for the MPEG-TS output.

## Why transcoding works

During a transcode, FFmpeg autorotates the decoded picture. The orientation is
baked into the output pixels and the frame dimensions swap, so the destination
container no longer needs the display matrix.

Only channels configured to copy video are affected. Forcing every such stream
to transcode would conceal the explicit copy choice and impose a potentially
large CPU or GPU cost, so ErsatzRS reports the mismatch through health instead.
The alternative is an fMP4 HLS stream, whose exact public override is listed in
the [HTTP endpoint reference](../reference/http-endpoints.md).
