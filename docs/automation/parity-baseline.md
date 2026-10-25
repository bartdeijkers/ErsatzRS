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
| **Pinned commit** | `f640c252276d008a5e1697fbfbb65a65fc352a5a` |
| **Commit subject** | `docs: prep for release v26.11.4` |
| **Commit URL** | https://github.com/ErsatzTV/legacy/commit/f640c252276d008a5e1697fbfbb65a65fc352a5a |
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

The maintainer fixed UCU-09's target on 2026-10-06 at upstream release
`v26.11.1`, commit `46118d43313b7828780fbfcfb46e980390ea8b0d`, ledger row208.
UCU-09 has **208 accepted and 0 pending in-scope rows**. This supersedes the
2026-09-19 moving-tip rule. Verify the release tag when refreshing upstream;
retain later commits as post-v26.11.1 work without extending or reopening UCU-09.
Fetching refs never advances the parity pin.

UCU-10 accepted rows209–232 through v26.11.2 and was delivered through PR105.
UCU-11 accepts rows233–236 through fixed v26.11.3 on2026-10-15: **236 accepted,
0 pending** across UCU-00–11. The Pin table advances with their reviewed ordered
dispositions after required local acceptance. Approved hardware/platform debt
remains explicitly unverified. Substantive source review uses exact git show
revisions; the shared upstream checkout is preserved to avoid disrupting other
work. Ordinary gap discovery still requires the checkout identity check above.
Remote application push and associated CI are authorized after local acceptance;
retry only failed specific CI jobs. Application release remains separate.

UCU-12 accepts rows237–254 through fixed v26.11.4 on2026-10-25:
**254 accepted,0pending** across UCU-00–12. The terminal tag/SHA and every
ordered subject are verified; required local gates and source-bound composed
CI recovery pass. Milestone ucu-12 owns exact evidence and retained negatives.
The maintainer permits closure with explicit unverified macOS/runtime and
hardware debt; supported targets remain. Latest delivery scope is CI and
source push/merge into remote/main only, without another release or later
catch-up work. The shared upstream checkout remains untouched.

At each start, print the pin, last processed ledger row (initial pin when no
rows are processed), and upstream HEAD. Stop on disagreement; do not
automatically checkout or change the pin to repair it. Then fetch the upstream target while
leaving HEAD pinned. Stop on fetch failure or non-prefix history instead of
silently using a stale target or rewriting processed rows.

The pin advances only through reviewed ledger steps, never through the daily
routine or an unreviewed baseline bump. The maintainer authorized ordered
batching of compatible rows201–208 on 2026-10-07; each row still needs its own
justified disposition and acceptance evidence. Land the filled rows and terminal
pin/subject/URL together only with maintainer commit authorization, then move
the clean upstream checkout to that SHA. Bootstrap leaves the pin intact. Milestone acceptance and evidence live in
`docs/progress/milestones.toml`; ADR 0095 accepts the conditional new-channel and
standalone-preview Next default with bundled or verified automatic acquisition.
The owning milestone records completed local acceptance and separate delivery status.

The active catch-up procedure uses direct substantive Codex review under the
maintainer's 2026-09-20 decision; Claude review is not a current gate. AMM-N-03
acceptance remains required before the first upstream step. Bootstrap review
and validation do not complete that prerequisite.
