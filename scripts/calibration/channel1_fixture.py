"""Five-minute SDR source and machine-readable visible counters for the VLC gate."""

import argparse
import importlib.util
import subprocess
from tempfile import TemporaryDirectory
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont
from channel1_timecode import FPS, FRAMES, write_tone

WIDTH, HEIGHT = 640, 360
FONT = ImageFont.load_default(size=27)


def counters(frame):
    seconds, subframe = divmod(frame, FPS)
    return f"{frame:04d}", f"{seconds // 60:02d}:{seconds % 60:02d}.{subframe * 100 // FPS:02d}"


def render(frame):
    image = Image.new("L", (WIDTH, HEIGHT), 24)
    draw = ImageDraw.Draw(image)
    for y, line in zip((10, 43), counters(frame)):
        for col, char in enumerate(line):
            draw.text((12 + 23 * col, y), char, font=FONT, fill=236)
    return image


def teletext(path):
    spec = importlib.util.spec_from_file_location(
        "fixture", Path(__file__).with_name("generate-teletext-fixture.py")
    )
    fixture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixture)
    transport = fixture.FixtureTransport()
    for tick in range(600):
        second = tick // 2
        value = str(second // 5 * 5).encode() if second >= 5 else b""
        transport.tables([(b"eng", 888)])
        # Finish the complete page at one presentation timestamp. FFmpeg aligns
        # each input's first PTS to zero. Repeat for viewers joining mid-cue.
        units = [
            fixture.data_unit(888, 0, fixture.page_header(888, True), True),
            fixture.data_unit(
                888, 20, fixture.page_row(b" " * 16 + b"\x0b\x0b" + value + b"\x0a\x0a"), True
            ),
            fixture.data_unit(899, 0, fixture.page_header(899)),
        ]
        transport.pes(units, 90000 + tick * 45000)
    path.write_bytes(transport.output)


def generate(ffmpeg, out):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=False)
    caption = out / "captions.ts"
    teletext(caption)
    with TemporaryDirectory(dir=out) as directory:
        tones = [Path(directory) / f"{hz}.wav" for hz in (750, 1500)]
        for path, hz in zip(tones, (750, 1500)):
            write_tone(path, hz)
        encode_clip(ffmpeg, out, caption, tones)


def encode_clip(ffmpeg, out, caption, tones):
    args = [
        ffmpeg,
        "-nostdin",
        "-v",
        "error",
        "-n",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "gray",
        "-s",
        f"{WIDTH}x{HEIGHT}",
        "-r",
        str(FPS),
        "-i",
        "pipe:0",
    ]
    for path in tones:
        args += ["-i", str(path)]
    args += [
        "-fix_teletext_pts",
        "0",
        "-i",
        str(caption),
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-map",
        "2:a",
        "-map",
        "3:s",
        "-vf",
        "scale=1920:1080:flags=neighbor,setsar=1",
        "-c:v",
        "libx264",
        "-threads:v",
        "4",
        "-preset",
        "ultrafast",
        "-crf",
        "17",
        "-g",
        str(FPS),
        "-bf",
        "0",
        "-pix_fmt",
        "yuv420p",
        "-bsf:v",
        "h264_metadata=colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1",
        "-color_primaries",
        "bt709",
        "-colorspace",
        "bt709",
        "-color_trc",
        "bt709",
        "-c:a",
        "ac3",
        "-b:a",
        "384k",
        "-metadata:s:a:0",
        "language=eng",
        "-metadata:s:a:1",
        "language=nld",
        "-c:s",
        "copy",
        "-t",
        "300",
        "-f",
        "mpegts",
        str(out / "Channel One (2026).ts"),
    ]
    with subprocess.Popen(args, stdin=subprocess.PIPE) as enc:
        try:
            for i in range(FRAMES):
                enc.stdin.write(render(i).tobytes())
        finally:
            enc.stdin.close()
        if enc.wait():
            raise RuntimeError("calibration encoder failed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    options = parser.parse_args()
    generate(options.ffmpeg, options.directory)
