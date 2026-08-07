# Graphics input fixtures

## Provenance and licensing

Both files are generated from FFmpeg's built-in `lavfi` sources. They contain no
third-party imagery, no trademarks, and no media from any library, so they are
redistributable under this repository's own licence with no attribution
requirement.

Regenerate with the shipped FFmpeg 9 runtime:

```bash
# animated-alpha.webp — 8 frames, 8 fps, 64x64, 50% alpha, infinite loop flag
ffmpeg -y -f lavfi -i "testsrc2=size=64x64:rate=8:duration=1" \
  -vf "format=rgba,colorchannelmixer=aa=0.5" \
  -c:v libwebp_anim -loop 0 -lossless 1 -frames:v 8 animated-alpha.webp

# static-alpha.webp — single frame, for the "still images are unchanged" case
ffmpeg -y -f lavfi -i "color=c=blue:size=64x64:duration=0.1" \
  -vf "format=rgba" -frames:v 1 -c:v libwebp static-alpha.webp
```

## Why these shapes

`animated-alpha.webp` is deliberately lossless with a uniform 50% alpha so a
test can assert exact compositing arithmetic: a source pixel `(127, 0, 254)`
composited over white decodes to `(189, 124, 253)`, which is the midpoint of
each channel and therefore proves the alpha channel survived the decode and the
overlay.

Its eight frames are all distinct, so a decoded frame-hash sequence
distinguishes three outcomes that otherwise look alike: correct looping (the
eight hashes repeat), a missing loop option (the last hash freezes), and a
broken decode (fewer than eight distinct hashes).

`static-alpha.webp` exists so the "more than one frame means animated" probe has
a negative case that is the same container and codec family.

## Measured behavior (FFmpeg 9.0)

- ffprobe `-count_frames` reports 8 for the animated file and 1 for the static
  one, which is exactly what `is_watermark_animated` keys on.
- The `webp_anim` demuxer defaults `ignore_loop` to **true**, so an animated
  WebP watermark needs `-ignore_loop 0` to repeat. Without it the final frame
  freezes for the rest of the programme.
