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
| **Pinned commit** | `0d301df5e8512fdfccf3a611188d27ab1690ee31` |
| **Commit subject** | `remove external dependencies (bugsnag, trakt) (#2840)` |
| **Commit URL** | https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31 |
| **Status** | Active baseline ("for now" — advance deliberately, see below) |

## Rules

1. **Scope of a "parity gap":** a behavior is only in scope if it exists in
   ErsatzTV **at the pinned commit**. Anything ErsatzTV adds upstream **after**
   this commit is **out of scope** until the pin is advanced — do not flag it as
   a gap, do not write a goal prompt for it, and do not regress a `Ported`
   feature because upstream moved on.
2. **Reading source:** when reading ErsatzTV under `$ERSATZTV_SRC/`, the routine
   must first ensure that checkout is at the pinned commit
   (`git -C "$ERSATZTV_SRC" rev-parse HEAD` equals the pin). If it is not, check
   it out (`git -C "$ERSATZTV_SRC" fetch && git -C "$ERSATZTV_SRC" checkout
   0d301df5e8512fdfccf3a611188d27ab1690ee31`) before evaluating gaps. If the
   checkout cannot be pinned, report that and **do not** emit parity changes
   from a mismatched tree.
3. **Goal prompts** must instruct the implementing agent to read ErsatzTV source
   at this commit, and must not introduce ParityDebt sourced from post-baseline
   upstream work.

## Advancing the pin

Bumping the baseline is an explicit, reviewed action — never automatic:

1. Edit the **Pinned commit** / **subject** rows above to the new SHA.
2. Re-run the routine; it will re-evaluate gaps against the new snapshot.
3. Record the bump in `docs/progress/sessions.ndjson`.

The daily routine must **not** advance this pin on its own.
