<!--
generated: 2026-07-17
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

Current selection: ADR 0021 French (France) localization closed on 2026-07-17
and was removed from the actionable queue. The next three maintainer-requested,
proposed admin-localization ADRs now lead in stable ADR-number order: German
(Germany), Hindi (India), and Indonesian (Indonesia).

---

## 1. Build the next ErsatzRS spec chore: ADR 0022 German (Germany) admin UI localization (new slug `admin-ui-de-de-localization`)

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

---

## 2. Build the next ErsatzRS spec chore: ADR 0023 Hindi (India) admin UI localization (new slug `admin-ui-hi-in-localization`)

Use the repo workflow from AGENTS.md. This is a spec-backed localization chore
governed by [ADR 0023](adr/0023-admin-ui-hi-in-localization.md) and ADR 0012's
translation boundary. Add first-class Hindi (India) support across the generated
language registry, every admin `TextKey` and dynamic helper, persisted language
selection, SSR initial language, and hydrated browser workflows. Keep technical
identifiers and user-authored media data verbatim.

Before editing, read ADR 0023, ADR 0012, `TEST.md`, and
`docs/how-to/testing.md`; map the language registry, aliases,
`pages.language` persistence, `/settings/ui`, and the existing i18n
Playwright proof. Register the progress slug when implementation starts. Do not
mark the ADR or progress feature implemented until all translations, coverage
tests, localized desktop/mobile screenshots, clean browser console proof,
`cargo xtask parity-check web`, and `cargo xtask progress check` pass.

---

## 3. Build the next ErsatzRS spec chore: ADR 0024 Indonesian (Indonesia) admin UI localization (new slug `admin-ui-id-id-localization`)

Use the repo workflow from AGENTS.md. This is a spec-backed localization chore
governed by [ADR 0024](adr/0024-admin-ui-id-id-localization.md) and ADR 0012's
translation boundary. Add first-class Indonesian (Indonesia) support across the
generated language registry, every admin `TextKey` and dynamic helper,
persisted language selection, SSR initial language, and hydrated browser
workflows. Keep technical identifiers and user-authored media data verbatim.

Before editing, read ADR 0024, ADR 0012, `TEST.md`, and
`docs/how-to/testing.md`; map the language registry, aliases,
`pages.language` persistence, `/settings/ui`, and the existing i18n Playwright
proof. Register the progress slug when implementation starts. Do not mark the
ADR or progress feature implemented until all translations, coverage tests,
localized desktop/mobile screenshots, clean browser console proof,
`cargo xtask parity-check web`, and `cargo xtask progress check` pass.
