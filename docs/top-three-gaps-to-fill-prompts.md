<!--
generated: 2026-09-22
parity_baseline_commit: a74896dfcdb425e4759ebb3cc17b1e1d9d8921cc
generator: reviewed catch-up pin advancement
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Selection reviewed current phase focus, runnable milestones, all open parity
debts, ADR implementation states and the live plan index. Upstream catch-up
ranks first by maintainer focus; existing add-on publication acceptance and
confirmed provider availability debt remain next. Teletext waits for UCU-09.
These prompts do not expand an active UCU task into independent add-on work.
Read the authoritative pin from `docs/automation/parity-baseline.md`; the
front-matter value is a generated checkpoint, not permission to reset a newer pin.

## 1. Build the next ErsatzRS spec chore: one upstream catch-up row (slug `upstream-baseline-catch-up`)

### Workflow

Follow AGENTS.md and `plans/upstream-catch-up.md`. Complete UCU-01 acceptance through `a74896df` before processing the first
pending row (`ea4d3b65`, UCU-02). Run the recorded full-window gates; exact-SHA
CI requires separate authorization. Only after UCU-01 is Complete may the
next one-row post-pin catch-up task begin.

### Before editing

Verify pin, last landed ledger row and upstream HEAD agreement. Fetch upstream
and verify/extend the ledger prefix as the plan requires. Read the complete
selected upstream diff and surrounding source at its SHA; map affected behavior
to native code/tests. ADR 0095 accepts Next as an optional second engine and
cannot justify the rejected native-only exclusions.

### Implement the slice

Use `skills/parity-slice.md` for applicable behavior. Choose one evidence-backed
verdict, implement every required part of that row, and preserve subtitle/audio
behavior. Record any missing behavior as visible owning-feature debt. Use the
existing per-row review and local landing procedure; stop after that row.

### Verification gates

Run `cargo xtask affected-checks`, the relevant focused `./run.sh test` and scoped
Clippy checks, and `cargo xtask progress check`. Documentation-only rows need
no invented runtime test. Playback and UI changes require their plan-defined
smoke/browser evidence, including source/visual mapping for any UI port.
Full-window gates and separately authorized exact-SHA CI remain mandatory at
the milestone boundary; a single landed row does not complete UCU-01.

### Reporting

Report the source map, upstream SHA/subject, verdict, changed files, verification,
remaining debt, old/new pin and next pending row. Do not infer push, CI or release
authorization from local commit permission.

## 2. Build the next ErsatzRS spec chore: close AMM-P-01 acceptance (slug `addon-metadata-management`)

### Workflow

Follow AGENTS.md, ADR 0094, `plans/automatic-addon-publication.md` and the full
AMM-P-01 milestone. Finish automatic-publication acceptance without stopgaps,
preserving locally playable media and operator state.

### Before editing

Query `cargo xtask progress milestones --id amm-p-01 --full`. Compare each open
clause with current source, exact candidate and reusable native Windows evidence.
Build a source map of publication, verification receipts and subtitle preparation;
identify which previously observed failures still lack proof. Do not treat
AMM-N-03 acceptance as automatic closure of this separate milestone.

### Implement the slice

Correct only demonstrated remaining failures. Preserve automatic publication,
metadata, overrides, replacements and local playback. Keep unproven acceptance
visible in the owning milestone and parity debt. Follow TEST.md and create a
Visual Source Map if a correction changes the admin workspace.

### Visual verification

For changed browser behavior, build a matched bundle with `./run.sh serve`, use
the existing workspace reference at the same viewport, and verify controls,
layout, native forms and hydration with the appropriate Playwright scenarios.

### Verification gates

Use `cargo xtask affected-checks` and focused `./run.sh test` during corrections.
Complete the unchanged AMM-P-01 closeout, playback and browser gates where valid
evidence is missing; preserve the required maintainer Windows acceptance report.
Run `cargo xtask progress check`; do not repeat valid evidence without cause.

### Reporting

Report the clause audit, exact artifact/source evidence, changed files and
remaining blockers. Keep build, CI, package and native acceptance claims separate.

## 3. Build the next ErsatzRS spec chore: correct cookie-enabled availability (slug `addon-metadata-management`)

### Workflow

Follow AGENTS.md and ADR 0094. Close only the open
`addon-ytdlp-needs-auth-despite-cookies` debt without stopgaps. This is an
ErsatzRS add-on contract defect, not an upstream pinned-baseline feature port.

### Before editing

Read the debt and inspect the official add-ons repository's current import and
enrichment availability mapping. Build a source map through provider results,
publication and the UI. Diagnose why successful cookie-enabled extraction with
playable formats can still map `needs_auth` to unavailable while Test access
reports accessible. Do not expose cookies, account details or private media.

### Implement the slice

Correct the provider mapping while retaining genuine authentication failures,
removed items and non-playable outcomes. Add focused synthetic regression cases
for successful extraction versus those failures. Preserve host provider neutrality.
Update the owning parity evidence and changelog; do not close the debt from a
mapper-only assertion if its required operator behavior remains unproven.

### Visual verification

Verify consistent access and availability in the existing admin workspace using
a matched bundle and disposable fixtures. If UI code changes, follow TEST.md,
create a Visual Source Map and compare same-viewport screenshots with the
canonical workspace design after `./run.sh serve`.

### Verification gates

Use the add-ons repository's documented focused test contract; run affected
host tests through `./run.sh test` when host code changes. Use the relevant
Playwright scenario for operator-visible consistency and
`cargo xtask progress check` for the owning record. Retain missing native
provider proof explicitly rather than weakening acceptance.

### Reporting

Report source mapping, causal diagnosis, corrected behavior, tests, changed
files and remaining debt. Repository publication and releases require their own
authorization and evidence.
