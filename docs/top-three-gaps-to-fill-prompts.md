<!--
generated: 2026-07-05
last_processed_commit: 0e2cba696aba6882aeec5ea284ee7465219321e6
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts, each closing one ErsatzTV parity gap,
ADR-backed runtime hardening gap, or spec-backed chore. These prompts are
selected from the current progress files, ADR index, active plans, and phase
state per [gap-prompt-rules.md](automation/gap-prompt-rules.md).

Current selection: carried over unchanged on 2026-07-05. The change window
(`f7afb3cc..0e2cba6`) was a single `fix(core)` commit that stabilized the ADR
0020 playout-rebuild verification and flipped ADR 0020 to
`Accepted 2026-07-03` / Implemented; it touched only already-`Ported`
`playout-rebuild-on-impacting-changes` code and docs, closed none of the three
published prompts, and introduced no new open debt, so no parity status
changed. The full candidate sweep is unchanged: one remaining whole-feature
parity/spec gap (`similar-video-cooldown`), the proposed admin localization ADRs
(0021–0047), the active release Docker plan
(`plans/docker-sequential-platform-publishing.md`), the module-split plan
(`plans/web-server-wasm-crate-split.md`), and deferred contract work. Applying
the selection rules, ADR 0019 leads because it is the only remaining non-ported
feature and is maintainer-requested scheduling behavior. The next two slots go
to the first proposed, maintainer-requested admin localization ADRs by stable
ADR-number order: French (France), then German (Germany). The release Docker
plan and module-split work remain actionable follow-ups after these higher
ranked spec chores unless phase focus changes.

---

## 1. Build the next ErsatzRS spec chore: ADR 0019 similar video cooldown scheduling (slug `similar-video-cooldown`)

Use the repo workflow from AGENTS.md. This is an ErsatzRS scheduling extension
governed by [ADR 0019](adr/0019-similar-video-cooldown.md), not an ErsatzTV
behavior clone; compare ErsatzTV only to preserve disabled-by-default imported
schedule behavior. The goal is a complete, opt-in, deterministic cooldown-aware
shuffle policy without stopgaps. Existing schedules must behave exactly as they
do today when the policy is disabled, and any shortfall must stay visible as ADR
or `ParityDebt`.

Tracked debt: `similar-video-cooldown-scheduling` in
[`docs/progress/parity/similar-video-cooldown.toml`](progress/parity/similar-video-cooldown.toml).
Current debt text: scheduling has no shared opt-in cooldown-aware shuffle policy
that can keep groupable media, such as the same artist, TV show, movie
franchise, parent variant, collection, tag, or operator-defined filter, apart
for a configured number of videos or playout duration. Reclose only after ADR
0019 is implemented with deterministic cooldown-aware selection, persisted
Continue-build state, disabled-by-default ErsatzTV parity behavior, focused
scheduler tests, and admin or schedule-definition controls for item-count and
duration cooldown thresholds.

Before editing:
1. Read ADR 0019, `STYLE.md`, `TEST.md`, and `docs/how-to/testing.md`.
2. Inspect the current scheduling model and engines:
   `crates/ersatzrs-core/src/scheduling/classic/`,
   `crates/ersatzrs-core/src/scheduling/block.rs`,
   `crates/ersatzrs-core/src/scheduling/yaml.rs`,
   `crates/ersatzrs-core/src/scheduling/scripted.rs`, playout anchor/context
   persistence, scheduling reference-data loading, and admin schedule editors.
3. Build a Source Map covering policy serialization, similarity-tag hydration,
   deterministic candidate selection, anchor/context persistence across
   `Continue`, and every UI or schedule-definition entry point.
4. Build a Visual Source Map for the admin schedule editor routes that will
   expose the cooldown policy.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Add versioned `SchedulingCooldownPolicy`, rule, and state models that are
  disabled by default and leave imported ErsatzTV schedules byte/behavior
  compatible when absent.
- Hydrate similarity tags before the scheduling hot loop for artist, album
  artist, show, season, ADR 0018 parent/variant lineage, collection/tag/genre or
  other already-loaded facets, and operator-defined search/filter rules.
- Add a shared deterministic soft-cooldown selector for shuffled/randomized
  scheduling units. It must avoid active groups when alternatives exist,
  degrade deterministically when every candidate is blocked, and keep multi-item
  units indivisible.
- Persist cooldown state in each engine's existing anchor/context path so
  `Continue` builds do not forget active cooldowns.
- Expose the policy in DB-backed admin schedule editors and in YAML/scripted
  schedule definitions using the same semantics; localize every new admin string.
- Add focused tests proving disabled policy parity, count-based and
  duration-based cooldowns, impossible-constraint relaxation, persisted
  `Continue` state, and at least one UI or schedule-definition save/load path.
- Update the parity TOML, ADR 0019 implementation text, `CHANGELOG.md`, this
  prompt file, and the session log. Do not mark the feature `Ported` until the
  selector, persistence, controls, and tests all land.

Visual verification:
- Serve via `./run.sh serve` after rebuilding Leptos assets.
- Capture the changed schedule-editor route(s) in desktop and mobile viewports.
- Verify the cooldown controls are hydrated, labels are localized, save/load
  round trips work, and the browser console is clean.

Verification:
- Focused scheduler tests for the new cooldown policy in `ersatzrs-core`.
- Focused persistence tests in `ersatzrs-infra` for anchor/context state.
- Focused web tests for admin schedule editor save/load behavior.
- `cargo xtask parity-check web`
- `cargo xtask progress check`

When done, report the Source Map, Visual Source Map, behavior implemented,
remaining ADR/`ParityDebt`, verification run, and changed files.

---

## 2. Build the next ErsatzRS spec chore: ADR 0021 French (France) admin UI localization (new slug `admin-ui-fr-fr-localization`)

Use the repo workflow from AGENTS.md. This is a spec-backed localization chore
governed by [ADR 0021](adr/0021-admin-ui-fr-fr-localization.md) and ADR 0012's
translation boundary, not a parity-slice port. The goal is first-class French
(France) admin UI support without stopgaps: every `TextKey`, dynamic helper,
settings option, persisted-language path, and browser proof must work before
the ADR is marked implemented.

Before editing:
1. Read ADR 0021, ADR 0012, `TEST.md`, and `docs/how-to/testing.md`.
2. Inspect `crates/ersatzrs-web/src/admin/i18n.rs`,
   `crates/ersatzrs-web/src/admin/languages/`,
   `crates/ersatzrs-web/build.rs`, `/settings/ui`, and
   `tests/e2e/settings-i18n.spec.ts`.
3. Build a Source Map covering the generated language registry, normalized
   culture aliases, `pages.language` persistence, server-side initial language
   loading, dynamic helper methods, and test coverage.
4. Build a Visual Source Map for `/settings/ui` and at least two translated
   admin route families already covered by the settings i18n proof.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Add French (France) as `fr-fr` with aliases `fr`, `fr-FR`, and `fr_FR`, and
  render the language option as `français (France)`.
- Add a dedicated admin language module following the existing per-culture
  contract and metadata shape.
- Translate every admin `TextKey` plus dynamic helper output covered by the
  `LanguageStrings` trait. Preserve technical identifiers, provider names,
  codec names, routes, configuration keys, logs, and user-authored media data
  verbatim per ADR 0012.
- Update i18n coverage tests and the settings i18n Playwright proof so French
  selection, `html lang`, route markers, and absence of English fallback text
  are verified.
- Register the new progress slug `admin-ui-fr-fr-localization` in the
  appropriate progress source when implementation starts, then mark it and ADR
  0021 implemented only after full coverage and browser proof.
- Update `CHANGELOG.md`, this prompt file, and the session log.

Visual verification:
- Serve via `./run.sh serve` after rebuilding Leptos assets.
- Capture `/settings/ui` and the route samples used by
  `tests/e2e/settings-i18n.spec.ts` in French.
- Verify labels fit at desktop and mobile widths, no English fallback markers
  remain where French translations are expected, and the browser console is
  clean.

Verification:
- `cargo test -p ersatzrs-web all_languages_translate_each_key`
- `cargo test -p ersatzrs-web supported_languages_match_requested_i18n_scope`
- `npx playwright test tests/e2e/settings-i18n.spec.ts --project=chromium`
- `cargo xtask hydration-lint -p ersatzrs-web`
- `cargo xtask parity-check web`
- `cargo xtask progress check`

When done, report the Source Map, Visual Source Map, translations added,
remaining ADR/progress debt, verification run, and changed files.

---

## 3. Build the next ErsatzRS spec chore: ADR 0022 German (Germany) admin UI localization (new slug `admin-ui-de-de-localization`)

Use the repo workflow from AGENTS.md. This is a spec-backed localization chore
governed by [ADR 0022](adr/0022-admin-ui-de-de-localization.md) and ADR 0012's
translation boundary, not a parity-slice port. The goal is first-class German
(Germany) admin UI support without stopgaps: every `TextKey`, dynamic helper,
settings option, persisted-language path, and browser proof must work before
the ADR is marked implemented.

Before editing:
1. Read ADR 0022, ADR 0012, `TEST.md`, and `docs/how-to/testing.md`.
2. Inspect `crates/ersatzrs-web/src/admin/i18n.rs`,
   `crates/ersatzrs-web/src/admin/languages/`,
   `crates/ersatzrs-web/build.rs`, `/settings/ui`, and
   `tests/e2e/settings-i18n.spec.ts`.
3. Build a Source Map covering the generated language registry, normalized
   culture aliases, `pages.language` persistence, server-side initial language
   loading, dynamic helper methods, and test coverage.
4. Build a Visual Source Map for `/settings/ui` and at least two translated
   admin route families already covered by the settings i18n proof.

Implement the coherent end-to-end slice that genuinely closes the gap:
- Add German (Germany) as `de-de` with aliases `de`, `de-DE`, and `de_DE`, and
  render the language option as `Deutsch (Deutschland)`.
- Add a dedicated admin language module following the existing per-culture
  contract and metadata shape.
- Translate every admin `TextKey` plus dynamic helper output covered by the
  `LanguageStrings` trait. Preserve technical identifiers, provider names,
  codec names, routes, configuration keys, logs, and user-authored media data
  verbatim per ADR 0012.
- Update i18n coverage tests and the settings i18n Playwright proof so German
  selection, `html lang`, route markers, and absence of English fallback text
  are verified.
- Register the new progress slug `admin-ui-de-de-localization` in the
  appropriate progress source when implementation starts, then mark it and ADR
  0022 implemented only after full coverage and browser proof.
- Update `CHANGELOG.md`, this prompt file, and the session log.

Visual verification:
- Serve via `./run.sh serve` after rebuilding Leptos assets.
- Capture `/settings/ui` and the route samples used by
  `tests/e2e/settings-i18n.spec.ts` in German.
- Verify labels fit at desktop and mobile widths, no English fallback markers
  remain where German translations are expected, and the browser console is
  clean.

Verification:
- `cargo test -p ersatzrs-web all_languages_translate_each_key`
- `cargo test -p ersatzrs-web supported_languages_match_requested_i18n_scope`
- `npx playwright test tests/e2e/settings-i18n.spec.ts --project=chromium`
- `cargo xtask hydration-lint -p ersatzrs-web`
- `cargo xtask parity-check web`
- `cargo xtask progress check`

When done, report the Source Map, Visual Source Map, translations added,
remaining ADR/progress debt, verification run, and changed files.
