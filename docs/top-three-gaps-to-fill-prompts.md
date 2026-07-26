<!--
generated: 2026-07-26
last_processed_commit: 2fffbb3452127aa922f449d4c382cbec40dd0a90
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Goal prompts selected from the current progress files, ADR index, active plans,
and Phase 7C state. p7c-02 (watermark preview) and p7c-03 (deferred search / See
All) both completed and flipped to `Ported` with every targeted debt closed, so
their prior prompts are retired. The only pinned-baseline parity gap left is the
Phase 8 jukebox, which is blocked on p7c-04 and the p8a-01 product contract and
is therefore not actionable today. The remaining actionable work is the runnable
p7c-04 module split (phase focus) plus the highest-ranked spec chores: the two
lowest-numbered unimplemented, maintainer-requested localization ADRs.

---

## 1. Build the next ErsatzRS spec chore: split large modules after boundary extraction (slug `application-use-case-layer`)

This is the runnable Phase 7C milestone **p7c-04** and the phase-focus chore.
Follow AGENTS.md and the P7C-04 contract in
[`plans/application-layer-and-video-jukebox.md`](../plans/application-layer-and-video-jukebox.md);
this is an ErsatzRS structural refactor with no pinned ErsatzTV behavior to port,
so do **not** force the parity-slice workflow. The goal is the complete,
behavior-preserving module split the plan describes, with no public-behavior
change and no weakened completeness gate. It unblocks p8a-01, so it ranks first.

Before editing:

1. Confirm `cargo xtask progress milestones --id p7c-04 --full` is runnable, then
   refresh the measurements in
   [`plans/module-split-survey.md`](../plans/module-split-survey.md) so the split
   targets reflect current file sizes, not stale ones.
2. Read the P7C-04 plan section and the
   [`web-server-wasm-crate-split.md`](../plans/web-server-wasm-crate-split.md)
   split plan, then inspect the reduced media-browser transport, the YAML and
   scripted scheduling registries, and the provider adapters named by the survey.
3. Build a Source Map of module ownership, dependency directions, exhaustive
   dispatch/match arms, tests, and the public transport/DTO contracts each module
   exposes.

Implement the complete slice:

- Split the surveyed modules along application, infrastructure, scheduler, and
  transport ownership boundaries, one file at a time, keeping each move
  behavior-preserving.
- Keep native application code out of the hydration/WASM path and preserve every
  public DTO, route, server-function signature, persisted shape, dispatch
  behavior, and provider semantic.
- Retain or strengthen the exhaustive-dispatch / completeness gates so a file
  move cannot silently drop a branch.
- Update p7c-04 evidence in `docs/progress/milestones.toml`, the active plans,
  `CHANGELOG.md`, `phase.toml`, this prompt file, and the session log.

Verification:

- `cargo fmt --all -- --check`
- focused crate tests through `./run.sh`
- `cargo xtask architecture-lint`
- both workspace clippy configurations (`--all-targets`, and again `--features mysql`)
- `./run.sh test-closeout`
- `cargo xtask parity-check web`
- `cargo xtask progress check`
- `git diff --check`

Report the refreshed survey measurements, the Source Map, the final module
ownership, the preserved contracts, verification results, changed files, and the
live next milestone. Do not commit or push unless explicitly requested.

---

## 2. Build the next ErsatzRS spec chore: German (Germany) admin UI localization (ADR 0022, no new parity slug)

Implement [ADR 0022](../docs/adr/0022-admin-ui-de-de-localization.md) (Status
`Proposed`, Implementation `Not implemented`) — a maintainer-requested admin UI
language. Follow AGENTS.md and ADR 0012's translation boundary
([`docs/adr/0012-admin-ui-nl-localization.md`](../docs/adr/0012-admin-ui-nl-localization.md)).
There is **no** pinned ErsatzTV behavior to port and **no** new parity slug: the
supported culture list intentionally diverges from ErsatzTV, so the chore extends
the file-driven language system and the `settings`/i18n coverage rather than
adding a `docs/progress/parity/<slug>.toml`. The goal is a fully translated,
selectable `de-de` culture with zero English fallback leakage outside intentional
technical values; any missing translation must be visible as ADR implementation
debt, not hidden in prose.

Before editing:

1. Read ADR 0022's Decision block and ADR 0012's boundary. Use ADR 0021's French
   implementation as the worked example.
2. Inspect the file-driven language system: the existing modules in
   `crates/ersatzrs-web/src/admin/languages/` (`en_us.rs`, `nl.rs`, `pl.rs`,
   `pt_br.rs`, `fr_fr.rs`, `mod.rs`), the `build.rs`-generated module registry and
   settings selector, `crates/ersatzrs-web/src/admin/i18n.rs`, and the complete
   set of admin `TextKey`s a language module must cover.
3. Build a Source Map of the language-module contract (metadata: code, label,
   aliases, sort order; the `LanguageStrings`/`TextKey` completeness set), the
   registry generation, the settings language selector, and the SSR shell
   language wiring.

Implement the complete slice:

- Add a dedicated `de_de.rs` admin language module covering **every** admin
  `TextKey`, following the metadata and completeness contract of the existing
  modules.
- Persist the normalized culture as `de-de`, render the option label as
  `Deutsch (Deutschland)`, and normalize `de`, `de-DE`, and `de_DE` to `de-de`.
- Apply ADR 0012's translation boundary: translate admin product text and stable
  workflow messages; keep technical identifiers, provider/codec names, routes,
  configuration keys, logs, and user-authored media data verbatim.
- Flip ADR 0022 to `Implemented` (with date) only once coverage is complete and
  the settings/i18n verification path proves `de-de` selects and renders.
- Update `CHANGELOG.md`, the `settings` parity coverage note, this prompt file,
  and the session log.

Visual verification:

- Build a matched SSR/WASM bundle and serve with `./run.sh serve`; select
  `Deutsch (Deutschland)` in settings and confirm the shell, settings routes, and
  a sampling of admin routes render translated headings, labels, and actions with
  no alert or hydration error and no English fallback outside intentional
  technical values.

Verification:

- `cargo fmt --all -- --check`
- `cargo xtask architecture-lint`
- focused tests through `./run.sh` (including any `TextKey` completeness gate)
- `cargo xtask hydration-lint -p ersatzrs-web`
- `cargo xtask parity-check web`
- extend/run `tests/e2e/settings-i18n.spec.ts` to cover `de-de` (Chromium and Firefox)
- both workspace clippy configurations
- `./run.sh test-closeout`
- `cargo xtask progress check`
- `git diff --check`

Report the Source Map, the coverage proof (every `TextKey` translated), the
selection/render evidence, the ADR status change, remaining debt if any,
verification results, and changed files. Do not commit or push unless explicitly
requested.

---

## 3. Build the next ErsatzRS spec chore: Hindi (India) admin UI localization (ADR 0023, no new parity slug)

Implement [ADR 0023](../docs/adr/0023-admin-ui-hi-in-localization.md) (Status
`Proposed`, Implementation `Not implemented`) — the next-numbered
maintainer-requested admin UI language. Follow AGENTS.md and ADR 0012's
translation boundary. As with ADR 0022 there is **no** pinned ErsatzTV behavior
to port and **no** new parity slug; the chore extends the file-driven language
system and `settings`/i18n coverage. The goal is a fully translated, selectable
`hi-in` culture with no English fallback leakage outside intentional technical
values; any shortfall must be visible as ADR implementation debt.

Before editing:

1. Read ADR 0023's Decision block and ADR 0012's boundary; use ADR 0021's French
   module and (if landed) the ADR 0022 German module as worked examples.
2. Inspect the same file-driven language system as ADR 0022: the modules under
   `crates/ersatzrs-web/src/admin/languages/`, the `build.rs`-generated registry
   and settings selector, `i18n.rs`, and the full admin `TextKey` set.
3. Build a Source Map of the language-module contract, registry generation,
   settings selector, and SSR shell language wiring.

Implement the complete slice:

- Add a dedicated `hi_in.rs` admin language module covering **every** admin
  `TextKey`.
- Persist the normalized culture as `hi-in`, render the option label as
  `हिन्दी (भारत)`, and normalize `hi`, `hi-IN`, and `hi_IN` to `hi-in`.
- Keep the admin layout left-to-right for this locale (no route/component
  direction change) per ADR 0023.
- Apply ADR 0012's translation boundary: translate admin product text and stable
  workflow messages; keep technical identifiers, provider/codec names, routes,
  configuration keys, logs, and user-authored media data verbatim.
- Flip ADR 0023 to `Implemented` (with date) only once coverage is complete and
  the settings/i18n verification path proves `hi-in` selects and renders.
- Update `CHANGELOG.md`, the `settings` parity coverage note, this prompt file,
  and the session log.

Visual verification:

- Build a matched SSR/WASM bundle and serve with `./run.sh serve`; select
  `हिन्दी (भारत)` in settings and confirm the shell, settings routes, and a
  sampling of admin routes render translated headings, labels, and actions with
  no alert or hydration error, the Devanagari script displays correctly, and no
  English fallback leaks outside intentional technical values.

Verification:

- `cargo fmt --all -- --check`
- `cargo xtask architecture-lint`
- focused tests through `./run.sh` (including any `TextKey` completeness gate)
- `cargo xtask hydration-lint -p ersatzrs-web`
- `cargo xtask parity-check web`
- extend/run `tests/e2e/settings-i18n.spec.ts` to cover `hi-in` (Chromium and Firefox)
- both workspace clippy configurations
- `./run.sh test-closeout`
- `cargo xtask progress check`
- `git diff --check`

Report the Source Map, the coverage proof, the selection/render evidence, the
ADR status change, remaining debt if any, verification results, and changed
files. Do not commit or push unless explicitly requested.
