<!--
generated: 2026-06-21
last_processed_commit: 24e8383a2155503890457543a82958248e406dba
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill - goal prompts

Actionable paste-and-run goal prompts, each closing **one** ErsatzTV parity gap
or ADR-backed parity-critical runtime hardening gap. Auto-prioritized from
`docs/progress`, with manual maintainer promotions called out inline.
Regenerated daily by the [progress routine](automation/daily-progress-routine.md);
see the [rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Current reconcile (2026-06-21): the HLS / MPEG-TS
`all-streams-in-use-playable-slate` closeout resolved the last item emitted by
`cargo xtask progress gaps`, and ADR 0008 FFmpeg runtime acquisition from
first-party `bartdeijkers/ErsatzRS-ffmpeg` releases is now implemented. Per
`docs/automation/gap-prompt-rules.md`, completed scope is replaced, never kept
with a completed banner. This file intentionally contains no actionable gap
prompt; run `cargo xtask progress gaps` before adding new work here.

---
