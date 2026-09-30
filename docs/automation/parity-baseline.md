# ErsatzTV parity baseline (pinned)

Parity gaps are measured against a **fixed ErsatzTV upstream snapshot**, not
against ErsatzTV's moving `main`. This pin is the single source of truth for
"how far ErsatzRS has to reach." The [daily progress
routine](daily-progress-routine.md) and every generated [goal
prompt](gap-prompt-rules.md) MUST honor it.

## Pin

| Field | Value |
| --- | --- |
| **Upstream repo** | `https://github.com/ErsatzTV/legacy` |
| **Pinned commit** | `558855083c7602b6f50ed63675ff61d1d325f98d` |
| **Commit subject** | `prep for release v26.10.0 [no ci]` |
| **Commit URL** | https://github.com/ErsatzTV/legacy/commit/558855083c7602b6f50ed63675ff61d1d325f98d |
| **Status** | Active baseline ("for now" — advance deliberately, see below) |

## Rules

These rules govern ordinary baseline gap discovery. Explicit catch-up tasks
use the exception below to inspect and reconcile one post-pin commit, with
behavioral debt in its owning parity record. They do not authorize the daily
routine to select post-pin work or advance the pin.

1. **Scope of a "parity gap":** a behavior is only in scope if it exists in
   ErsatzTV **at the pinned commit**. Anything ErsatzTV adds upstream **after**
   this commit is **out of scope** until the pin is advanced — do not flag it as
   a gap, do not write a goal prompt for it, and do not regress a `Ported`
   feature because upstream moved on.
2. **Reading source:** when reading ErsatzTV under `$ERSATZTV_SRC/`, the routine
   must first ensure that checkout is at the pinned commit
   (`git -C "$ERSATZTV_SRC" rev-parse HEAD` equals the **Pinned commit** row in
   the table above). Outside catch-up mode, if it is not, check it out
   (`git -C "$ERSATZTV_SRC" fetch && git -C "$ERSATZTV_SRC" checkout <Pinned
   commit>`) before evaluating gaps. Always read the SHA from that table, never
   from a copy elsewhere: catch-up advances the pin one row at a time, so any
   duplicate goes stale and would silently pin the tree to an older snapshot. If
   the checkout cannot be pinned, report that and **do not** emit parity changes
   from a mismatched tree.
3. **Goal prompts** must instruct the implementing agent to read ErsatzTV source
   at this commit, and must not introduce ParityDebt sourced from post-baseline
   upstream work.

## Advancing the pin

Bumping the baseline is an explicit, reviewed action — never automatic:

1. Edit the **Pinned commit** / **Commit subject** / **Commit URL** rows above to the new SHA.
2. Re-run the routine; it will re-evaluate gaps against the new snapshot.
3. Record the bump in `docs/progress/sessions.ndjson`.

The Pin table is the only authoritative record of the current pin. Do not copy
that value into rules, prompts, plans or parity records; reference the table
instead. The catch-up ledger's fixed `Initial pin` is a different, historical
anchor for the ledger range and stays as written.

The daily routine must **not** advance this pin on its own.

## Catch-up mode

The latest fetched target is `76884783996ac3aa11a232622939b8bd03ed0d86` (verified 2026-09-30),
currently **181 commits** beyond the initial pin. Per the maintainer's
2026-09-19 amendment, every run fetches upstream `origin/main` and appends new
commits to the [catch-up ledger](../../plans/upstream-catch-up.md); the original
`495e3799` cutoff no longer limits scope. The latest fetched tip is the run's
target. Refreshing the target never advances the parity pin.

At each start, print the pin, last processed ledger row (initial pin when no
rows are processed), and upstream HEAD. Stop on disagreement; do not
automatically checkout or change the pin to repair it. Then fetch the upstream target while
leaving HEAD pinned. Stop on fetch failure or non-prefix history instead of
silently using a stale target or rewriting processed rows.

The pin advances only through reviewed ledger steps, never through the daily
routine or a bulk baseline bump. Land the filled row and updated pin/subject/URL
together only with maintainer commit authorization, then move the upstream
checkout to that SHA. Stop after one upstream commit in each task run;
bootstrap leaves the pin intact. Milestone acceptance and evidence live in
`docs/progress/milestones.toml`; ADR 0095 accepts Next as an optional second engine; implementation remains open.

The active catch-up procedure uses direct substantive Codex review under the
maintainer's 2026-09-20 decision; Claude review is not a current gate. AMM-N-03
acceptance remains required before the first upstream step. Bootstrap review
and validation do not complete that prerequisite.
