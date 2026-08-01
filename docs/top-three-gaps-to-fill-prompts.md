<!--
generated: 2026-08-01
last_processed_commit: 30341464d78c1b455a97479708f84cf18a6a36d2
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Goal prompts selected from the current progress files, ADR index, active plans,
and Phase 8A state. The `a4ee0ca..3034146` window (the prior daily-reconcile doc
commit plus two work commits) landed **delayed mirror channel playback**
(`fix(infra): honor delayed mirror playback` `c4b7eba`, then `test(web):
stabilize delayed mirror browser proof` `3034146`): delayed/generated mirror
channels now reuse their source channel's playout across MPEG-TS, HLS
(direct + segmenter), browser preview, admin EPG, and XMLTV while keeping the
mirror's own channel settings (FFmpeg profile, branding, selectors, watermarks,
subtitle/audio preferences). Media is resolved from the generated source at
`wall clock - PlayoutOffset` and item/programme times are projected back onto
the target timeline; signed offsets round-trip correctly (negative offsets no
longer decode to zero), source history is retained until the largest positive
dependent mirror offset elapses, source rebuilds invalidate dependent mirror
sessions, a new Mirror Channels health check reports missing/non-Generated
sources and malformed/out-of-range offsets and legacy mirrors that also own a
playout, and `/playouts/add` disables a mirror backed by a source playout so it
cannot receive unrelated programming. This is grounded ErsatzTV parity at the
pin (`ChannelPlayoutSource.Mirror`, `Channel.MirrorSourceChannelId`).

The work commits self-reconciled their own docs: `channels-admin-ui.toml`'s
`mirror-channel-runtime-ignored-source-and-offset` debt is now `resolved`
(runtime evidence via Chromium/Firefox `tests/e2e/channel-watch.spec.ts`), and
`mpeg-ts-channel-streaming`, `hls-variant-playlists-segmenter`, `xmltv-guide`,
`playout-list-detail`, and `ersatzrs-healthcheck` summaries plus `hydration.toml`
note the source-playout projection. All affected features were already `Ported`,
so this run confirmed those edits, verified **no** parity status flip is
warranted, appended the daily session line, and refreshed the front-matter. None
of the merged work touched the top three chores.

`cargo xtask progress gaps` still reports a single remaining feature
(`video-jukebox-addon-api`, Not started, four open debt items) — the only file
with unresolved `[[debt]]` anywhere in `docs/progress/parity/` (the mirror debt
closed this window). Its first slice is milestone **p8a-01**, the `phase.toml`
`current_focus` decision gate that unblocks p8a-02/03 and p8b-01/02, so it holds
the top slot. Every other actionable spec source is the queue of 26 `Proposed` /
`Not implemented` localization ADRs (0022–0047); no ADR-backed FFmpeg/HLS
hardening or blocking runtime debt is open to outrank them (the delayed-mirror
work closed with no open debt), and the only deferred plan work (mdBook, macOS
archives, Claude hooks) is developer-experience tooling that ranks below the
maintainer-requested localization ADRs. The two lowest-numbered of those
(ADR 0022 de-de, ADR 0023 hi-in) saw no commits this window and carry over
unchanged as `#2` and `#3`.

---

## 1. Build the next ErsatzRS spec chore: decide the video-jukebox product and add-on contract (slug `video-jukebox-addon-api`)

This is the runnable Phase 8A milestone **p8a-01** and the `phase.toml`
`current_focus` chore. Follow AGENTS.md and the P8A-01 contract in
[`plans/application-layer-and-video-jukebox.md`](../plans/application-layer-and-video-jukebox.md).
This is a deliberate ErsatzRS product/architecture decision with **no** pinned
ErsatzTV behavior to port, so do **not** force the parity-slice workflow. The
goal is one **accepted** ADR that decides the whole Phase 8 jukebox contract so
that schema, application, and transport work can start; the ADR must leave no
listed decision undecided, and any deferral must be recorded as visible ADR
implementation debt, not hidden in prose. It unblocks p8a-02, p8a-03, p8b-01,
and p8b-02 — the entire remaining feature-complete port gap — so it ranks first.

Before editing:

1. Confirm `cargo xtask progress milestones --id p8a-01 --full` is runnable and
   read the `blocking_decision` note in
   [`docs/progress/phase.toml`](progress/phase.toml). Do **not** begin schema,
   public-API, or jukebox implementation in this slice — the deliverable is the
   decision record only.
2. Read the P8A-01 plan section (product and architecture decisions), the Phase
   8B ingress/proof sections, and the four open `[[debt]]` items in
   [`docs/progress/parity/video-jukebox-addon-api.toml`](progress/parity/video-jukebox-addon-api.toml)
   (`jukebox-contract-is-undecided`,
   `jukebox-has-no-durable-request-domain`,
   `jukebox-has-no-application-scheduler-integration`,
   `jukebox-has-no-public-addon-ingress-or-runtime-proof`).
3. Review the accepted application-boundary decision (ADR 0051) and the existing
   `ApplicationServices`/operation-coordinator seams so the add-on transport
   choice is consistent with the composition root, and confirm the plan's
   recommendation of an out-of-process HTTP/event boundary over a native
   dynamic-plugin ABI.

Implement the complete slice (a decision record, not code):

- Author a new ADR (next free number) that decides every P8A-01 item: channel
  scope and eligible music-video sources; requester identity, moderation,
  authorization, and privacy; FIFO/priority policy, duplicate and
  artist/video cooldowns, expiry, and per-source rate limits; current-item
  behavior, safe insertion boundaries, cancellation, retries, filler
  interaction, and failure recovery; and the stable add-on transport/versioning
  with an explicit out-of-process vs. in-binary adapter decision.
- Register the ADR in the ADR index, set its `video-jukebox-addon-api` parity
  linkage, and record the request lifecycle states the plan lists (Pending,
  Accepted, Rejected, Queued, Playing, Played, Cancelled, Expired, Failed) as
  the contract the durable domain (p8a-02) must implement.
- Update `docs/progress/parity/video-jukebox-addon-api.toml` (add the `adr`
  number; refine `jukebox-contract-is-undecided` toward resolved only when the
  ADR is accepted), `docs/progress/milestones.toml` p8a-01 evidence,
  `phase.toml`, `CHANGELOG.md`, this prompt file, and the session log.

Verification:

- `cargo xtask progress adr list` (new ADR shows with correct status)
- `cargo xtask progress check`
- `git diff --check`

Report the decided contract summary, the open questions resolved vs. any
deferred as visible debt, the transport decision and its rationale, the parity
linkage, verification results, changed files, and the live next milestone. Do
not commit or push unless explicitly requested.

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
