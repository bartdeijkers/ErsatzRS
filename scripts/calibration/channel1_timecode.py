"""Frame-locked Manchester amplitude code for the two channel-one carriers."""

import hashlib
import wave

import numpy as np

RATE = 48_000
FPS = 25
FRAMES = 300 * FPS
BLOCK = 192  # Four milliseconds, three or six complete carrier cycles.
CODE_ID = "channel-one-manchester-v1"


def chip_levels():
    bits = np.frombuffer(hashlib.shake_256(CODE_ID.encode("ascii")).digest(FRAMES), dtype="u1") & 1
    # Each frame contains both levels, with the bit encoded by their order.
    high = np.column_stack((bits, 1 - bits)).ravel()
    return np.where(high, 1.0, 10 ** (-0.8 / 20))


def reference_envelope():
    levels = chip_levels()
    blocks = np.repeat(levels[:, None], 5, axis=1)
    blocks[:, 0] = (levels + np.roll(levels, 1)) / 2
    return blocks.ravel()


def write_tone(path, hz):
    levels = chip_levels()
    previous = np.roll(levels, 1)
    size = RATE // (2 * FPS)
    ramp = np.minimum(np.arange(size) / BLOCK, 1.0)
    carrier = 32767 * 10 ** (-18 / 20) * np.sin(2 * np.pi * hz * np.arange(size) / RATE)
    with wave.open(str(path), "wb") as audio:
        audio.setparams((2, 2, RATE, 0, "NONE", "not compressed"))
        for i in range(0, len(levels), 2 * FPS):
            lo = previous[i:i + 2 * FPS, None]
            hi = levels[i:i + 2 * FPS, None]
            mono = np.rint((lo + (hi - lo) * ramp) * carrier).astype("<i2")
            audio.writeframesraw(np.repeat(mono.ravel(), 2).tobytes())
