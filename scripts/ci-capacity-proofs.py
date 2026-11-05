#!/usr/bin/env python3
"""Capacity proofs: CPU-bound budgets that only hold on runners with more than four CPUs.

A standard GitHub-hosted runner has four vCPUs. The cases below prove a fixed
real-time budget (not functional behavior) and cannot meet it there, so CI runs
them only where the runner has more than four CPUs and reports them as skipped
elsewhere. Their budgets are never relaxed; the self-hosted runners and the
nightly full run on main keep proving them.

Usage:
  ci-capacity-proofs.py capable            exit 0 when this runner can run them
  ci-capacity-proofs.py nextest-exclude    nextest filter that leaves them out
  ci-capacity-proofs.py browser-grep       Playwright --grep-invert pattern
  ci-capacity-proofs.py report next|browser   Markdown note for the job summary
"""

import os
import re
import sys

MIN_CPUS = 5

NEXT_TESTS = (
    "next_engine::process::runtime_tests::late_pgs_runtime_tests::"
    "real_next_4k_truehd_starts_before_first_pgs_and_preserves_late_picture_audio",
)

BROWSER_TITLES = (
    "finding 09 decodes compact JPEG XL in a full native broadcast graph",
)


def available_cpus():
    try:
        return len(os.sched_getaffinity(0))
    except AttributeError:
        return os.cpu_count() or 1


def capable(cpus=None):
    return (available_cpus() if cpus is None else cpus) >= MIN_CPUS


def nextest_exclude():
    return "not (" + " | ".join(f"test(={name})" for name in NEXT_TESTS) + ")"


def browser_grep():
    return "|".join(re.escape(title) for title in BROWSER_TITLES)


def report(kind, cpus=None):
    cases = NEXT_TESTS if kind == "next" else BROWSER_TITLES
    cpus = available_cpus() if cpus is None else cpus
    lines = [f"### Capacity proofs skipped ({cpus} CPUs, more than {MIN_CPUS - 1} required)", ""]
    lines += [f"- `{case}`" for case in cases]
    lines += ["", "Their real-time budgets are not relaxed: a self-hosted run or the nightly full run proves them."]
    return "\n".join(lines) + "\n"


def main(argv):
    if len(argv) == 1 and argv[0] == "capable":
        return 0 if capable() else 1
    if len(argv) == 1 and argv[0] == "nextest-exclude":
        print(nextest_exclude())
        return 0
    if len(argv) == 1 and argv[0] == "browser-grep":
        print(browser_grep())
        return 0
    if len(argv) == 2 and argv[0] == "report" and argv[1] in ("next", "browser"):
        sys.stdout.write(report(argv[1]))
        return 0
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
