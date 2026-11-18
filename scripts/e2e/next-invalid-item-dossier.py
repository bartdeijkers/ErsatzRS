"""Exercise pre-encoder troubleshooting with the exact candidate worker."""

import argparse
import json
from datetime import datetime, timedelta, timezone
from pathlib import Path
import shutil
import subprocess
import tempfile


def check(worker, ffmpeg, ffprobe, reports_output):
    with tempfile.TemporaryDirectory(prefix="next-invalid-item-") as directory:
        root = Path(directory)
        for name in ("playout", "output", "reports"):
            (root / name).mkdir()
        config = {
            "version": "https://ersatztv.org/channel/version/0.1.6",
            "playout": {"folder": str(root / "playout")},
            "ffmpeg": {"ffmpeg_path": str(ffmpeg), "ffprobe_path": str(ffprobe),
                       "reports_folder": str(root / "reports")},
            "normalization": {"audio": {"format": "aac"}, "video": {"format": "h264", "bit_depth": 8}},
        }
        config_file = root / "channel.json"
        config_file.write_text(json.dumps(config))
        now = datetime.now(timezone.utc).replace(microsecond=0)
        start, finish = now - timedelta(minutes=5), now + timedelta(minutes=5)
        item = {"id": "row305", "start": start.isoformat(timespec="milliseconds"),
                "finish": finish.isoformat(timespec="milliseconds"),
                "source": {"source_type": "invalid_fixture_type"}}
        playout = {"version": "https://ersatztv.org/playout/version/0.0.8", "items": [item]}
        filename = f"{int(start.timestamp()) * 1000}_{int(finish.timestamp()) * 1000}.json"
        (root / "playout" / filename).write_text(json.dumps(playout))
        result = subprocess.run([worker, "run", config_file, "--output-folder", root / "output",
                                 "--number", "305", "--troubleshoot"], timeout=90)
        if result.returncode <= 0:
            raise ValueError(f"expected an invalid-item error exit, got {result.returncode}")
        reports = root / "reports"
        inputs = list(reports.glob("*/playout_item.json"))
        if len(inputs) != 1 or json.loads(inputs[0].read_text()) != item:
            raise ValueError("the dossier must preserve the invalid scheduled item")
        outcome = (inputs[0].parent / "outcome.txt").read_text()
        if item["id"] not in outcome or "invalid_fixture_type" not in outcome:
            raise ValueError(f"unexpected troubleshooting outcome: {outcome}")
        if list(reports.rglob("ffreport.log")):
            raise ValueError("the invalid item unexpectedly produced an encoder report")
        shutil.copytree(reports, reports_output)
        print("Invalid-item dossier verified before FFmpeg encoding")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ("worker", "ffmpeg", "ffprobe", "reports-output"):
        parser.add_argument("--" + option, required=True, type=Path)
    check(**vars(parser.parse_args()))
