<!--
generated: 2026-06-22
last_processed_commit: 596a2fc08fee8cf416c3c95167a54ae88c8ae195
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts, each closing **one** ErsatzTV parity gap,
ADR-backed runtime hardening gap, or **spec-backed chore** (the next slice of a
`Proposed` / `Not implemented` ADR, an active plan, or an open phase TODO).
Auto-prioritized across `docs/progress`, the ADR index, `plans/`, and
`phase.toml` per the [rules](automation/gap-prompt-rules.md); regenerated daily
by the [progress routine](automation/daily-progress-routine.md).

`cargo xtask progress gaps` is **one** input, not the whole backlog: it inspects
only whole-feature parity status and goes clear long before the specs are fully
implemented. The chores below come from the wider spec sweep (unimplemented
ADRs, active plans, open phase TODOs) that the routine now performs.

**Parity baseline:** ErsatzTV *parity* chores are measured against ErsatzTV
commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31).
Spec-backed chores below are deliberate ErsatzRS decisions and may have no
ErsatzTV equivalent; each cites its governing ADR/plan/TODO.

Selection (2026-06-22): `cargo xtask progress gaps` is clear and every tracked
`[[debt]]` is `resolved`, so the top three come from the spec backlog. The
2026-06-22 reconcile window (`7901e3a..596a2fc`) landed only docs/CI/runtime
fixes — ADR 0008 managed FFmpeg acquisition is now implemented (with a Windows
stale-bundled-path follow-up fix) and the release line intentionally drops the
macOS archives — so **no commit touched the three chores below; all three are
carried over unchanged and remain actionable today**. Ranking:
(1) **content-variants** — named in `phase.toml` `next_steps` as the next
capability and backed by an explicit maintainer request (ADR 0018), so it leads;
(2) **ersatzrs-healthcheck binary** — open `phase.toml` `[[known_todo]]`,
release/runtime hardening that every container deployment depends on; (3)
**in-app TSDuck MPEG-TS troubleshooting** — ADR 0013, `Proposed` /
`Not implemented` (only the manual script gate shipped; the `/system/
troubleshooting/mpeg` Support tool is unbuilt).

---

## 1. Build the next ErsatzRS spec chore: special-features / alternate-versions / stream-roles, Phase 0 foundation (creates slug `content-variants`)

Use the repo workflow from AGENTS.md. This is a **spec-backed chore**, not an
ErsatzTV parity port: it is governed by
[ADR 0018](adr/0018-extras-alternate-versions-and-stream-roles.md) and
[`plans/extras-alternate-versions-and-stream-roles.md`](../plans/extras-alternate-versions-and-stream-roles.md).
Do **not** force `skills/parity-slice.md`; there is no pinned ErsatzTV behavior
to port. The goal is the fully working **Phase 0 — Foundation** slice (schema +
search contract) with no stopgaps; any shortfall must be recorded as visible
`ParityDebt` in the new parity file, not hidden in prose.

Scope is **only Phase 0** from the plan's "Phases" section — do not attempt the
UI, scanner-import, alternate-version linking, stream-role, or scheduling
phases. Build exactly the data-model and search-contract foundation those later
slices depend on.

Before editing:
1. Read ADR 0018 and the plan in full; reread the plan's **Data Model**,
   **Search Contract**, and **Phase 0** sections.
2. Read the ErsatzTV files the plan's **Source Map** names under `$ERSATZTV_SRC/`
   (`LocalFolderScanner.cs` `ExtraFiles`/`ExtraDirectories`,
   `MovieFolderScanner.cs` filter call site) so the schema can later *reverse*
   the discard — but do not change scanner behavior in this slice.
3. Inspect the current ErsatzRS schema/migrations, search index, and
   `docs/progress/` to confirm no `content-variants` slug exists yet.
4. Identify the `ParityDebt` you are deferring to later phases.

Implement the Phase 0 slice end-to-end:
- Add the migration(s) and model fields the Data Model section specifies
  (subordinate/alternate linkage, e.g. `ParentMediaItemId`, role/kind columns),
  following STYLE.md SQL rules (EF-style `PK_/FK_/IX_` names, identifier casing).
- Wire the Search Contract additions so extras/alternate-versions/roles are
  *representable and queryable*, even though no scanner imports them yet.
- Register the new parity slug: create
  `docs/progress/parity/content-variants.toml` (status `In progress`, accurate
  `summary`, `adr = [18]`, structured `[[debt]]` for every later phase) and add
  any health-check touchpoint required by the Health Check Parity contract.
- Add focused tests for the schema/search additions.

Verification (backend/pipeline — no UI in this slice):
- Focused crate tests for the new schema/search code.
- `cargo xtask progress check` — must pass with the new slug registered.
- `cargo test --workspace --all-features` for the touched crates.

Report the Source Map, the Phase 0 behavior implemented, the `ParityDebt` left
open for Phases 1–5, the verification run, and changed files.

---

## 2. Fix the next ErsatzRS parity-infrastructure gap: purpose-built `ersatzrs-healthcheck` binary

Use the repo workflow from AGENTS.md. This is an open hardening TODO recorded in
`docs/progress/phase.toml` (`[[known_todo]]`: "Replace bash /dev/tcp HEALTHCHECK
with a purpose-built ersatzrs-healthcheck binary"). The goal is a real, working
healthcheck binary that the container image uses, with no stopgaps; record any
shortfall as visible debt in `phase.toml`, not in prose.

Before editing:
1. Read the current Docker `HEALTHCHECK` and the bash `/dev/tcp` probe it runs,
   plus the `/healthz` handler in `ersatzrs-web`.
2. Inspect `ARCHITECTURE.md` for crate-boundary rules and how the existing
   binaries (`ersatzrs-web`, `ersatzrs-scanner`) are structured.
3. Confirm the supported-platform matrix in AGENTS.md — the binary must build on
   all six targets (no hard-coded OS/arch outside `cfg` guards).
4. Identify the debt: a bash/`/dev/tcp` probe is fragile, shell-dependent, and
   not present on minimal images.

Implement the slice end-to-end:
- Add an `ersatzrs-healthcheck` binary (smallest crate that fits the workspace
  layout) that probes the server's health endpoint and exits 0/non-0.
- Point the Docker `HEALTHCHECK` at the new binary; drop the bash `/dev/tcp`
  probe.
- Keep it dependency-light and static-binary friendly (single static binary is a
  core project goal).
- Add a focused test for the probe's success/failure exit codes.
- Tick or update the `phase.toml` `[[known_todo]]` once it is genuinely closed.

Verification (backend — no UI):
- `cargo build --workspace` and the focused binary test.
- `cargo clippy --workspace --all-targets -- -D warnings` (mirror CI; also the
  `--features mysql` pass).
- `cargo xtask progress check`.

Report what changed in the image and workspace, the probe behavior, the
verification run, and changed files.

---

## 3. Build the next ErsatzRS spec chore: in-app TSDuck MPEG-TS troubleshooting at `/system/troubleshooting/mpeg`

Use the repo workflow from AGENTS.md. This is a **spec-backed chore** governed by
[ADR 0013](adr/0013-tsduck-mpeg-troubleshooting.md) (`Proposed` /
`Not implemented`) and
[`plans/mpeg-troubleshooting-tsduck.md`](../plans/mpeg-troubleshooting-tsduck.md).
Only the **manual** `scripts/e2e/tsduck-playback-gate.sh` gate exists today; the
**in-app** Support tool the ADR/plan describe is unbuilt. Do not force
`skills/parity-slice.md` — TSDuck is an intentional ErsatzRS diagnostic
extension with no ErsatzTV equivalent (confirmed by the ADR's `$ERSATZTV_SRC`
search). The goal is the fully working Support tool with no stopgaps; normal
IPTV/HLS/HDHomeRun/FFmpeg/startup paths must not depend on TSDuck.

Before editing:
1. Read ADR 0013 and the plan; reread the plan's first-target section for
   `/system/troubleshooting/mpeg` and the structured-report requirements
   (PSI/SI, PAT/PMT/service/PID, continuity counters, PCR validity, bitrate).
2. Inspect the existing `/system/troubleshooting` Support surface and its
   server functions in `ersatzrs-web` so the new tool reuses that page's
   structure, styling, and i18n boundary (ADR 0012).
3. Build a Source Map (route, server fn, TSDuck spawn, report model, archive)
   and a light Visual Source Map against the existing Support page.
4. Identify `ParityDebt` (e.g. platforms where TSDuck is absent — degrade
   gracefully, never crash the page).

Implement the slice end-to-end:
- Add the `/system/troubleshooting/mpeg` route + server function that spawns
  TSDuck on a bounded sample of a selected channel's TS output (ADR 0003 CLI
  spawn rules), parses it into a structured report, and offers the archive.
- Detect missing TSDuck at runtime and surface a clear "not installed" state
  rather than failing; keep all normal streaming paths TSDuck-free.
- Localize new user-facing strings for every supported culture
  (`crates/ersatzrs-web/src/admin/languages/`), per the Language Contract.
- Follow TEST.md hydration rules; add focused tests for report parsing and the
  not-installed fallback.

Visual verification (UI route):
- Rebuild Leptos assets, serve via `./run.sh serve`, open
  `/system/troubleshooting/mpeg`, and compare its layout to the existing
  `/system/troubleshooting` Support page (same containers, spacing, typography).
- Do not claim parity complete with a visible mismatch; record it as
  `ParityDebt`.

Verification:
- `cargo xtask parity-check web` and focused `ersatzrs-web` tests.
- Any added Playwright spec (`npx playwright test ...`).
- `cargo xtask progress check`.

Report the Source Map, Visual Source Map, behavior implemented, remaining
`ParityDebt`, verification run, and changed files.
