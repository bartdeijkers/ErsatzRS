"""Print a Markdown comparison of archived channel 1 playback measurements."""

import argparse
import sys
from pathlib import Path


def measurements(path):
    rows = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if not line.startswith("|") or len(cells) != 5 or "." not in cells[0]:
            continue
        metric, unit, acceptance, observed, verdict = cells
        if verdict not in {"PASS", "FAIL", "NIET GEMETEN"}:
            continue
        if metric in rows:
            raise ValueError(f"{path}: duplicate measurement {metric}")
        rows[metric] = (unit, acceptance, f"{verdict} · {observed}")
    if not rows:
        raise ValueError(f"{path}: no channel 1 measurement table")
    return rows


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reports", nargs="+", type=Path)
    args = parser.parse_args()
    runs = [measurements(path) for path in args.reports]
    header = ["Metric", "Unit", "Acceptance"] + [
        path.stem.removesuffix("_channel-one-test-results") for path in args.reports
    ]
    print("| " + " | ".join(header) + " |")
    print("| " + " | ".join(["---"] * len(header)) + " |")
    # Keep differing units or limits on separate rows across report versions.
    keys = dict.fromkeys((metric, *row[:2]) for run in runs for metric, row in run.items())
    for metric, unit, acceptance in keys:
        cells = [metric, unit, acceptance]
        for run in runs:
            row = run.get(metric)
            cells.append(row[2] if row and row[:2] == (unit, acceptance) else "—")
        print("| " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
