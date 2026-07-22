<!--
generated: 2026-07-22
last_processed_commit: ffa814070d3f406edb3b846ab7d8e8f62407ea38
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts, each closing one ErsatzTV parity gap,
ADR-backed runtime hardening gap, or spec-backed chore. These prompts are
selected from the current progress files, ADR index, active plans, and phase
state per [gap-prompt-rules.md](automation/gap-prompt-rules.md).

Current selection: the `8984fd5..ffa8140` window (2026-07-22 reconcile) landed
the 2026-07-21 structural stub/stopgap audit and its first hardening slices.
Smart Shuffle playback order was wired into playout building, manual playout
rebuilds were made reliable behind a shared lock, FFmpeg profile "transcode
audio/video" terminology was named explicitly (ADRs 0049/0050), the
application-boundary architecture lint was introduced (ADR 0051), structural
milestone tracking was added (`docs/progress/milestones.toml`), and playback
data-access failures were made explicit with a new health check. Each feature
commit reconciled its own parity file in the same change, so no additional
status flip is owed this run; the audit reopened cross-cutting Phase 7 work and
`phase.toml` now sets **current_focus = "Phase 7A — Guardrails and failure
semantics"** with `next_steps` explicitly sequencing milestones **p7a-03**,
**p7a-04**, then Phase 7B's **p7b-01**.

That phase focus supersedes the prior admin-localization ADR queue: with
`cargo xtask progress gaps` now reporting 8 reopened features and confirmed
runtime-visible debt (settings/source/library failures rendering as valid
empty state), the localization ADRs 0022–0024 drop below the guardrail
milestones and are removed. The three picks below are the exact chores
`phase.toml next_steps` names, ranked phase-focus-first: p7a-03 leads because it
carries the blocking "server errors render as empty/default state" debt; p7a-04
is the paired Phase 7A hardening milestone; p7b-01 is the foundational Phase 7B
application-service slice that later vertical migrations depend on.

---

## 1. Fix the next ErsatzTV parity gap in ErsatzRS: admin load, empty, and error-state semantics (slug `admin-ui-failure-semantics`)

Use the repo workflow from AGENTS.md plus `skills/parity-slice.md`. This closes
milestone **p7a-03** (`plans/application-layer-and-video-jukebox.md`). The goal
is fully functional ErsatzTV parity without stopgaps: an operational or schema
failure behind an admin resource must become a visible, actionable error state,
never a plausible empty collection or default form. Any shortfall must be
recorded as visible `ParityDebt`, not hidden in prose.

Lead with the blocking runtime debt, quoted from the parity file:

> `server-errors-render-as-empty-or-default-state` — "Admin pages frequently
> call server functions with `.await.unwrap_or_default()`, so settings, source,
> library, scheduling, and troubleshooting failures can render as plausible
> defaults or empty data." Evidence: 149 textual `await ... unwrap_or_default()`
> occurrences under `crates/ersatzrs-web/src/admin`; the settings parity history
> already records a real schema error that silently rendered defaults.

The paired debt `admin-row-decode-defaults-hide-invalid-data` (typed infra read
models) is partly shared with p7b-05; carry only the read-model work this slice
touches and keep the debt entry open for the rest — do not defer it behind an
"if in scope" hedge.

Before editing:
1. Read ErsatzTV source at the pinned baseline commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31` under `$ERSATZTV_SRC/`, following
   the feature's `etv_source`: `$ERSATZTV_SRC/ErsatzTV/Pages/` and
   `$ERSATZTV_SRC/ErsatzTV.Application/` handler result and validation flows —
   how ErsatzTV distinguishes loading, ready, genuinely-empty, and error states.
2. Inspect `crates/ersatzrs-web/src/admin/pages`,
   `crates/ersatzrs-web/src/admin/server_fns`, and the `crates/ersatzrs-infra`
   read paths; grep the `unwrap_or_default()` call sites.
3. Build a Source Map covering pages, server functions, the row-decode paths,
   the reusable state primitives you will add, and intentional deviations.
4. Build a Visual Source Map for the first coherent route family you migrate
   (e.g. settings or a media-source list), recording ErsatzTV's empty vs error
   presentation and the ErsatzRS shared primitives/CSS to reuse.

Implement the coherent end-to-end slice:
- Add shared localized Loading, Ready, Empty, and Error states and replace the
  silent server-error defaults across the first coherent route families.
- Make typed query rows preserve decode failures instead of fabricating empty
  identifiers, strings, or `false` values.
- Preserve the real, valid empty-state UX where a resource is legitimately
  empty; only failures become error states.
- Follow `TEST.md` hydration rules; add focused tests plus a Playwright proof
  that an induced server error renders the error state, not empty data.
- Update `admin-ui-failure-semantics.toml` (status/summary/debt), milestone
  p7a-03, `CHANGELOG.md`, this prompt file, and the session log.

Visual verification:
- Serve via `./run.sh serve` after rebuilding Leptos assets; capture the
  migrated route family in ready, empty, and error states at desktop and mobile
  widths and compare against the ErsatzTV reference.

Verification:
- `cargo xtask parity-check web`
- `cargo xtask hydration-lint -p ersatzrs-web`
- `npx playwright test` for the added error-state spec (name it in your report)
- `cargo xtask progress check`

When done, report the Source Map, Visual Source Map, behavior implemented,
remaining `ParityDebt`, verification run, and changed files.

---

## 2. Build the next ErsatzRS spec chore: Phase 7A development-route and tracker hardening (slug `developer-surface-hardening`)

Use the repo workflow from AGENTS.md. This is a spec-backed Phase 7 hardening
chore governed by [`plans/application-layer-and-video-jukebox.md`](../plans/application-layer-and-video-jukebox.md)
and milestone **p7a-04** — it has **no ErsatzTV runtime equivalent**, so do not
force the parity-slice workflow or invent an `etv_source`. The goal is the fully
working behavior the plan describes for this slice, with any shortfall recorded
as visible `ParityDebt`.

Scope, from the two open debts in `developer-surface-hardening.toml`:

> `development-routes-are-production-visible` — "The component gallery and Big
> Buck Bunny FFmpeg stream route are unconditionally registered in the normal
> application router." Evidence: `admin/routes.rs` registers `/dev/components`;
> `host/router.rs` registers `/dev/stream/bbb.ts`, whose handler reads
> repository test data and launches FFmpeg.

> `per-page-e2e-index-has-stale-todos` — "TEST.md duplicates route verification
> state and still marks several routes TODO after structured hydration and
> Playwright coverage landed; the playout row also mixes browser-smoke evidence
> with a real missing durable workflow spec."

Before editing:
1. Read the plan section covering development-surface hardening and milestone
   p7a-04 acceptance (`cargo xtask progress milestones --id p7a-04 --full`).
2. Inspect `crates/ersatzrs-web/src/admin/routes.rs`,
   `crates/ersatzrs-web/src/host/router.rs`, `TEST.md`,
   `docs/progress/hydration.toml`, and `xtask/src/progress.rs`.
3. Build a Source Map of the dev-route registrations, the TEST.md per-page
   table, and the structured hydration/route evidence it duplicates.

Implement the coherent end-to-end slice:
- Require an explicit non-production opt-in for the `/dev/components` gallery
  and `/dev/stream/bbb.ts` sample stream so neither registers in a normal
  production router; keep them available under the opt-in for development.
- Reconcile TEST.md so structured route/hydration sources are canonical and the
  stale TODO rows agree with `hydration.toml` and existing Playwright coverage.
- Add the missing durable playout-workflow browser test the playout row is
  standing in for, so browser-smoke evidence is no longer conflated with it.
- Correct the misleading scanner/show notes named in the milestone acceptance.
- Update `developer-surface-hardening.toml` (status/summary/debt), milestone
  p7a-04, `CHANGELOG.md`, this prompt file, and the session log.

Verification:
- `cargo xtask parity-check web`
- `npx playwright test` for the new playout-workflow spec (name it in your report)
- `cargo xtask progress check`

When done, report the Source Map, behavior implemented, which debts closed,
remaining `ParityDebt`, verification run, and changed files.

---

## 3. Build the next ErsatzRS spec chore: application service foundation (slug `application-use-case-layer`)

Use the repo workflow from AGENTS.md. This is an ADR-backed architectural chore
governed by [ADR 0051](adr/0051-application-boundary-and-composition-root.md)
and [`plans/application-layer-and-video-jukebox.md`](../plans/application-layer-and-video-jukebox.md),
closing milestone **p7b-01** (its dependency p7a-01 is complete). Read
`$ERSATZTV_SRC/ErsatzTV.Application/` at the pinned baseline commit
`0d301df5e8512fdfccf3a611188d27ab1690ee31` only where it informs the
command/query boundary — this is a deliberate ErsatzRS structural extraction,
not a UI parity port, so do not move SQL out of infrastructure. The goal is the
complete foundation ADR 0051 promises for this slice, with any shortfall
recorded as visible `ParityDebt`.

Scope is the foundational debt that later vertical slices (p7b-02..p7b-05)
depend on, quoted from `application-use-case-layer.toml`:

> `core-leaks-concrete-storage-error` — "ersatzrs-core::repositories::
> RepositoryError contains sqlx::Error, so the core trait boundary depends on
> the concrete persistence implementation." Evidence:
> `crates/ersatzrs-core/src/repositories/error.rs` exposes
> `Database(#[from] sqlx::Error)`, contrary to the storage-neutral core rule in
> ARCHITECTURE.md.

Also advance, per the milestone acceptance, the split-coordination debt
`operation-coordination-is-split` only as far as constructing injected
`ApplicationServices` requires; keep the remaining lock/queue unification open
for p7b-02/p7b-03 rather than hedging it away.

Before editing:
1. Read ADR 0051, the plan's application-boundary section, `ARCHITECTURE.md`,
   and milestone p7b-01 acceptance (`cargo xtask progress milestones --id
   p7b-01 --full`).
2. Inspect `crates/ersatzrs-app`, `crates/ersatzrs-core/src/repositories`,
   `crates/ersatzrs-infra`, and the web composition root that builds `AppState`.
3. Build a Source Map of the current core error type, the infra call sites that
   would map concrete failures, and the native composition root.

Implement the coherent end-to-end slice, per the acceptance criteria:
- Give `ersatzrs-app` documented command, query, and application-error modules.
- Stop `ersatzrs-core` from exposing `sqlx::Error`; make infra map concrete
  storage failures into the storage-neutral core/application error boundary.
- Have the native composition root construct injected `ApplicationServices`.
- Keep endpoint names, persisted shapes, side effects, and ErsatzTV behavior
  stable; do not rewrite features horizontally in this slice.
- Add focused regressions and keep `cargo xtask architecture-lint` green.
- Update `application-use-case-layer.toml` (summary/debt/adr), milestone p7b-01,
  `CHANGELOG.md`, this prompt file, and the session log.

Verification:
- `cargo xtask architecture-lint`
- `./run.sh check -p ersatzrs-app -p ersatzrs-core -p ersatzrs-infra`
- focused crate tests for the new error mapping and composition root
- `cargo xtask progress check`

When done, report the Source Map, behavior implemented, which debts closed or
advanced, remaining `ParityDebt`, verification run, and changed files.
