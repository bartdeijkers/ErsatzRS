# Domain-scoped pull request CI

> **Status:** Implementation started as the active maintainer goal on 2026-11-06.
> **Outcome:** Make ordinary code PRs merge-ready in approximately 15–30 minutes
> by selecting the builds and tests their changes actually require.
> **Authorization:** The original task wrote this plan only. The subsequent
> active goal authorizes its implementation. On 2026-11-06 the maintainer also
> authorized a second Codex review, fixes, commit/push, a PR and full candidate CI.
> Merge and publication remain separate; the performance cohort requires real PRs.
> **Baseline:** Repository `ce01d498`; Actions evidence inspected on 2026-11-06.

## 1. Purpose and ownership

The maintainer should receive a short, explainable PR check that covers the
changed behavior and its affected consumers. Presentation-only changes should
run relevant Playwright tests in Chromium, without unrelated Rust test jobs.
Teletext browser tests should run only when teletext behavior is affected.
Release-candidate CI must retain Chromium and Firefox and all other required
release evidence.

This is an engineering follow-up to the measured CI delays, not Phase 10
implementation or a reopening of accepted Phase 9 delivery. The current focus
was checked with `cargo xtask progress next`. This plan does not replace the
existing release, platform, or teletext acceptance contracts.

Canonical sources:

- [CI lanes and release-candidate gate](../BUILD.md#ci-lanes).
- [Release-candidate acceptance](../BUILD.md#release-candidate-ci-gate).
- [Web module boundaries](../docs/reference/web-crate-structure.md).
- [Architecture and crate dependencies](../ARCHITECTURE.md).
- [Testing and hydration constraints](../TEST.md).
- [Delivery and completion rules](../AGENTS.md#delivery-accountability).

The numbered work packages below describe intended implementation, not delivered
behavior. The `dci` milestone in [structured progress](../docs/progress/README.md)
owns the six package IDs, acceptance and evidence. Query it with
`cargo xtask progress milestones --id dci --full`. Keep completion status there;
do not maintain a second live checklist here. Phase 9 acceptance and the
separate Phase 10 task boundary remain unchanged.

## 2. Measured problem

The investigation examined 40 PR workflow runs from 2026-11-04 through
2026-11-06: 11 succeeded, 2 failed and 27 were cancelled. Six successful code
runs with a single attempt averaged 139.6 minutes, ranging from 112.1 to 153.6.
Documentation skips and reruns were excluded from that baseline calculation.
The sample contains many CI and teletext changes and is not a prediction for
the future PR mix.

| Example | Runner pool | Workflow creation to successful `CI ok` |
|---|---|---:|
| [PR #118: teletext failure continuation](https://github.com/bartdeijkers/ErsatzRS/actions/runs/42116633851) | Self-hosted | 112.1 min |
| [PR #110: teletext admin recovery](https://github.com/bartdeijkers/ErsatzRS/actions/runs/42020525558) | GitHub-hosted | 150.6 min |
| [PR #117: CI changes](https://github.com/bartdeijkers/ErsatzRS/actions/runs/41999174326) | GitHub-hosted | 153.6 min |
| [PR #119: documentation](https://github.com/bartdeijkers/ErsatzRS/actions/runs/42181052209) | Hosted preflight only | 14 sec |

The other three runs in the six-run code baseline are
[41897576328](https://github.com/bartdeijkers/ErsatzRS/actions/runs/41897576328),
[41873499583](https://github.com/bartdeijkers/ErsatzRS/actions/runs/41873499583) and
[41849741508](https://github.com/bartdeijkers/ErsatzRS/actions/runs/41849741508).

For PR #118, Preflight took 10 seconds, build 20.2 minutes, general Rust tests
6.2 minutes, Next 23.7 minutes and teletext Rust 7.0 minutes. The browser jobs
took 28.6 minutes for general and 49.1 minutes for Level 1.5. Those two jobs ran
sequentially. Their browser-lock waits were both zero: most of the 78 minutes
was execution, not lock contention. Some other jobs overlapped; these times
must not all be added together.

Evidence-backed causes:

1. A changed `transport/iptv/teletext_output.rs` file was classified as shared
   admin UI, selecting every general browser spec.
2. Teletext browser selection is a boolean, selecting the entire Level 1.5
   suite rather than the affected functions. The last run passed 62 cases and
   skipped 14. The general suite passed 363 cases and skipped 5.
3. Ordinary PR build flags do not independently follow Rust and browser demand.
   The separate smoke server alone took about 6.5 minutes to build in #118;
   its selected Next job did need it, but not every PR needs that product.
4. Hosted builds took 57–59 minutes in #110/#117. One run spent 12 minutes
   installing build tools. Runner and cache differences matter to the target.
5. Shared-runner scheduling also adds delay. In #118 the general browser job
   started about seven minutes after its dependencies completed. An earlier
   run waited approximately 39 minutes before its first local job.

Post-merge duplicate suppression already worked for #110, #117 and #118.
For example, the [#118 merge run](https://github.com/bartdeijkers/ErsatzRS/actions/runs/42131793841)
ran only Preflight and `CI ok`, citing the successful PR tree. Preserve that
behavior rather than treating post-merge retesting as the primary bottleneck.

## 3. Scope and exclusions

### Included

- Refine existing path/domain selection in `scripts/ci-domains.py`.
- Separate required build products from required test execution.
- Map crates, web submodules and explicit shared dependencies to test domains.
- Select general and teletext Playwright specs independently.
- Make `CI ok` validate the selected jobs and intentional omissions.
- Preserve release/full-run coverage, identity checks and runner safeguards.
- Measure the resulting critical path using existing Actions evidence.

### Excluded

- A new CI platform, general code-analysis engine or test-impact service.
- Splitting or renaming crates, including the separately proposed
  [server/WASM crate split](web-server-wasm-crate-split.md).
- Product changes, database migrations, test deletions, weaker assertions,
  shorter required observation windows or added retries to obtain green runs.
- Automatically moving an affected regression to nightly to meet a time limit.
- Increasing browser workers or removing same-host browser serialization.
- Paid runners, new hardware, macOS resumption, native ARM provisioning,
  runtime upgrades, release publication or changes to platform deferrals.
- A broad cache redesign, cross-revision binary reuse or linker experiment.
- Reworking release packaging to run a second copy of pre-tag browser tests.

The complete required Level 1/1.5 release boundary remains intact. Experimental
Level 2.5 stays available through its existing optional dispatches; this work
neither removes those checks nor promotes them to mandatory acceptance.

## 4. Selection contract

### 4.1 Classify by behavior, then by crate

Crates establish dependency boundaries, but `ersatzrs-web` includes frontend,
server functions, APIs and playback. It cannot be classified as frontend-only.

| Change class | PR test execution | Required build products |
|---|---|---|
| Reviewed presentation-only page, CSS or browser script | Relevant Chromium specs plus applicable route/hydration smoke | Matched SSR/WASM site and required E2E helpers |
| Presentation-only teletext page | Relevant Level 1.5 teletext Chromium specs plus applicable smoke | Same web bundle and helpers |
| Browser spec or helper only | Changed specs and affected helper consumers in Chromium | Web bundle and helpers; no unrelated Rust test archive |
| Server function, request validation, API or persistence | Affected Rust packages/consumers and relevant E2E contract cases | Selected Rust archive; web bundle only when E2E is selected |
| Scanner, scheduling or add-on service | Affected Rust packages/consumers; E2E only for mapped affected flows | Products consumed by those selected jobs |
| Teletext engine or output transport | Affected Rust, transport/playback and teletext browser families | Selected Rust archive, runtime helpers, smoke server/site as needed |
| Shared foundation or build/test infrastructure with unknown reach | Conservative full required selection | Full existing products |
| Documentation only | Existing documentation lane semantics | No application/test build |

"Only Playwright" describes test execution, not the absence of builds,
formatting, linting, artifact verification or lightweight repository checks.
Do not remove these safeguards as a side effect of skipping Rust tests.

Presentation-only eligibility must be explicit and reviewed. Start with CSS,
known browser scripts and audited presentation modules. Do not mark all
`admin/pages/**/*.rs` eligible: pages may contain server calls, validators,
data transformations or native tests. Mixed files retain their Rust coverage.
If a formerly presentation-only module gains server behavior or changes its
dependency shape, invalidate the narrow classification and select broader
checks. A filename, extension or PR label is not sufficient evidence.

Use existing module boundaries and reviewed mapping data. Do not implement a
general Rust call graph or AST-based semantic change detector for this plan.
If a mixed module cannot safely be narrowed, keep its broader selection and
record that limitation; a product refactor is not a prerequisite here.

### 4.2 Explicit domain and dependency map

Keep one canonical mapping in the existing selector, with independently
testable entries for:

- Libraries/scanning and provider connections.
- Media/collections/search and add-on management.
- Channels/playouts/scheduling.
- Graphics, watermarks and FFmpeg profiles.
- Settings, Health and troubleshooting.
- Teletext authoring/publication, preview/editor, and runtime/delivery/recovery.
- Shared shell, routing, localization and hydration.
- Native playback/transport and Next integration.

For backend changes, retain the existing reverse dependency closure for Rust
packages. This plan does not promise safe Rust test-function selection inside
every large crate. Expanding Rust build/test dependencies must not automatically
select every UI domain in the resulting crates.

Each selected domain needs a reason: changed path, direct owner, affected
consumer, explicit shared dependency, or conservative fallback. Use exact module
ownership and known dependency edges rather than broad keyword coincidences.
Resolve shared Playwright helpers through direct and transitive consumers;
unknown/dynamic imports or missing fixture ownership select a safe superset.

### 4.3 Teletext boundaries

Remove selection based solely on membership in a crate that happens to contain
teletext. A library-page change must not select teletext merely because both
live in `ersatzrs-web`; an unrelated repository change must not do so merely
because `ersatzrs-infra` contains teletext tests.

Use separate browser families, initially at whole-spec granularity:

| Family | Initial source/spec anchors to audit | Required dependent behavior |
|---|---|---|
| Authoring and publication | `admin/pages/channels/teletext_drafts*`, teletext server functions; `teletext-authoring-flow`, `teletext-effective-authoring-flow` | Validation, save, explicit publication and required base fallback |
| Preview and editor | Teletext rendering/editor handlers and helpers; `teletext-editor-flow` | Actual preview output, editor interaction and applicable shared authoring invariants |
| Runtime, delivery and recovery | `ersatzrs-ffmpeg/src/teletext`, web IPTV teletext output/retention, teletext runtime services; `teletext-restart-flow`, `channels-source-teletext-delivery`, `teletext-scripted-health` | Output lifecycle, recovery, Health, subtitle/audio safety and affected playback engines |

These anchors are a starting inventory, not an accepted exhaustive map. Package
DCI-01 must enumerate real owners and cross-family dependencies. Shared teletext
model/validation/publication changes select every affected family. Preserve
serial groups and fixture setup/teardown; do not split a group by title to save
time. Test-case-level selection is outside this first implementation.

Directly affected teletext also includes demonstrated shared dependencies:
hydration, routing, schema or transport changes may affect teletext without
having its name in their path. Such selection must explain that dependency.
For uncertain shared reach, retain the current full required selection until a
bounded mapping is proven. This is an explicit safety exception to narrow
selection, not permission for every web change to trigger teletext.

Do not select Next merely because any FFmpeg file changed. Map native-only,
Next-specific and shared pipeline contracts separately, preserving all applicable
audio, subtitle and output safety checks when a shared path is affected.

### 4.4 Diff and selection integrity

- Classify the complete PR diff against its base, not only the last commit.
  Include old and new paths of renames, deletions and added files. Retain API
  pagination and validate any truncated/incomplete diff before allowing skips.
- Test the PR merge result and record its identity. Base movement must not
  reuse obsolete selection or evidence for a different merged tree.
- The selection is the union of all changed-file requirements; adding a file
  must never remove a previously required check.
- New or unmapped production files, manifests, shared schemas, CI definitions
  and ambiguous dependencies select a conservative superset with a reason.
- Empty or malformed selector output fails closed. Distinguish an intentional
  no-Rust-tests selection from a filter that accidentally matches zero tests.
- An existing `.png`/Markdown ignore must not silently bypass runtime assets
  or embedded content. Limit documentation skipping to known documentation
  ownership and classify served assets or executable fixtures appropriately.
- Known required general/Level 1.5 tests retain owners; optional Level 2.5
  remains separate. Verify selected filters against actual discovered tests.

## 5. Build products and workflow behavior

Extend the existing selection output, rather than adding a second selector.
It must describe domains/reasons, Rust package/filter demand, general and
teletext spec lists, runtime jobs and required products. Exact field names are
an implementation detail; missing fields must never default to permission to
skip required work.

Derive product demand from consumers:

- **Rust archive:** only when a selected Rust/Next/teletext job consumes it;
  include all packages those jobs need, not just directly edited packages.
- **Matched web bundle:** only for browser/hydration consumers; keep the server
  and WASM build consistent. Frontend-only PRs still need a server to test.
- **Ordinary smoke server:** only for selected runtime smoke consumers. In
  the current workflow this is consumed by the Next smoke steps.
- **Helpers/runtimes:** retain `xtask`, provisioning and scanner executables
  wherever selected checks or E2E fixtures actually use them. A browser-only
  scan test still needs its scanner implementation. The implementation audit
  confirms that ordinary `helpers/scanned-library.ts` fixtures call the host's
  in-process `host/scanning.rs` worker. They do not spawn the scanner companion.
  The optional external teletext hold separately requires the exact supplied
  scanner executable; retain that prerequisite and its explicit opt-in.

The existing Rust artifact contract requires a smoke server unconditionally.
Update producer and verifier together if that requirement becomes conditional:
declare capabilities, require the consumer's capabilities and reject missing
payloads. Do not simply remove a filename from the verifier to make a smaller
archive pass. Preserve source identity, digest, build flags, platform and
matched-site checks. No new automatic reuse across source revisions.

Wire job dependencies to the actual selected producers. A browser-only PR may
proceed after its web build and required lightweight checks; a mixed PR retains
the existing rule that relevant Rust tests pass before browsers run. `CI ok`
must check an explicit required-job set and accept only intentional skips.
Failures, cancellations, absent selection and accidentally skipped required
jobs remain failures. Draft PRs must not become merge-ready accidentally.

PRs use Chromium for the selected browser specs. Nightly/full dispatch continues
to use the complete required general/Level 1.5 selection in both browsers.
Release-candidate verification uses that full lane on the exact candidate SHA
before tagging, as documented in BUILD.md. The tag packaging workflow is not
currently the owner of those browser tests; this plan preserves the pre-tag
gate rather than adding duplicate browser execution to `release.yml`.

Keep browser workers, same-host locking, capacity-proof eligibility and current
runner fallback unchanged. Fix omission of mandatory evidence by recording the
missing suitable runner proof, not by claiming a skipped capacity test passed.

## 6. Bounded implementation packages

Execute these in order. Finish and record each component before expanding into
the next. A checkpoint does not claim overall latency acceptance.

| ID | Requirement and smallest change | Evidence and stopping condition |
|---|---|---|
| DCI-01 | Define the reviewed frontend/backend/domain ownership map and finite selection fixtures using existing modules. Record explicit shared edges and unmapped cases. | Real file inventories and the regression cases in section 7 have expected selections. Mixed modules are identified. No workflow behavior changes. |
| DCI-02 | Implement explainable domain/spec selection in `ci-domains.py`, including teletext families, browser-only eligibility and safe fallback. Extend the existing selector tests. | Exact selection/omission assertions, union/rename/deletion cases, helper reach and actual spec inventory checks pass. Each omitted domain has a justified unaffected boundary. |
| DCI-03 | Make build flags and artifact capabilities follow selected consumers. Adapt `ci-build-artifacts.py` and its existing tests with the smallest compatible change. | Browser-only selection creates no Rust test archive or unused smoke server; backend-only selection creates no unused site. Required scanner/runtime helpers and artifact rejection tests pass. |
| DCI-04 | Connect selection, conditional products, browser spec filters and `CI ok` in `ci.yml`. Preserve full/release and draft behavior. | Workflow-condition tests prove required jobs cannot silently skip, browser-only jobs can run without a Rust test job, PR uses Chromium, and full uses both browsers with unchanged required coverage. |
| DCI-05 | Prove narrow-selection correctness and integration, update canonical build/test documentation and record implementation evidence. | The bounded validation matrix below passes, unchanged required full CI is green for the candidate, and the independent selection review has no unresolved omission. Component implementation may then close. |
| DCI-06 | Measure merge-ready latency over the fixed observation cohort in section 8. | Report all cohort entries, averages, queue/build/test breakdown and target result. Claim the performance outcome only if measured; otherwise identify the dominant blocker and a separately bounded next proposal. |

Primary implementation files are the existing three CI selector/artifact scripts,
their tests and `.github/workflows/ci.yml`. Change BUILD.md, TEST.md and
CHANGELOG.md only to describe delivered behavior. Do not edit product source
to make these packages easier. If a new behavior/architecture decision is
required, use the repository ADR process; this plan itself is not an ADR.

## 7. Verification and regression matrix

### Selector and workflow checks

Add cases to the existing Python suites rather than creating a separate testing
framework. Each positive selection needs a corresponding unrelated-domain
omission check where applicable.

| Input | Required proof |
|---|---|
| Library-page CSS/presentation | Library Chromium specs and applicable smoke; no Rust tests, Next or teletext solely through crate membership |
| Library server function or repository | Relevant Rust consumers and library E2E; not presentation-only |
| Changed scanner E2E helper/spec, as in #109 | Importing specs plus smoke; in-process scanner and any explicit external scanner prerequisites retained; no unrelated Rust archive |
| Teletext editor presentation | Editor/affected-family Chromium specs; no unrelated general admin suite or Rust job solely from its crate |
| Teletext output change, as in #118 | Output/recovery and relevant playback safety; no blanket general-admin classification |
| Shared teletext model, validator or publication contract | All affected teletext families and Rust consumers; no dropped base fallback |
| Shared hydration or route contract | Every affected browser domain, including teletext where it depends on that contract |
| Native-only versus shared Next/FFmpeg change | Correct distinct runtime selections, with audio/subtitle regression coverage retained |
| New file, rename across domains, deletion, unknown helper or multi-domain PR | Complete union or conservative fallback; no disappearing required domain |
| Manifest, schema, build/CI configuration | Full required selection; no false frontend-only shortcut |
| Documentation-only versus served image/embedded content | Correct different ownership; no extension-only skip of application content |
| Empty output, unmatched required filter, missing capability or wrong artifact identity | Explicit failure, not a passing empty suite |
| Draft, ready PR, merged-tree skip and full release candidate | Correct gate state, test identity and browser matrix |

Use the existing suites as applicable to the changed files:

```bash
python3 scripts/ci-domains_test.py
python3 scripts/ci-build-artifacts_test.py
python3 scripts/ci-teletext-selection_test.py
python3 scripts/ci-capacity-proofs_test.py
python3 scripts/e2e/playwright-e2e_test.py
git diff --check
```

These are planned implementation checks, not claims that they were executed
while writing this plan. Review the workflow's existing Python checks as well;
keep their ownership intact. Use available workflow validation tooling without
installing new infrastructure just for this task.

### Bounded integration evidence

Use three representative narrow scenarios: frontend-only, backend-only and
teletext transport. Reuse existing fixtures. Compare their proposed selections
with the full test inventory, including fixture dependencies and serial groups.
Demonstrate that each scenario's targeted behavioral regression is detected;
use existing negative/mutation evidence when valid rather than inventing a new
product defect. A selector unit test alone does not prove behavioral coverage.

Replay historical file lists from #109, #110, #117, #118 and #119 through the
new selector without rerunning all historical workflows. Review changes in
selection explicitly. Shared/CI changes may correctly remain broad.

The CI implementation PR itself changes shared infrastructure and must retain
its broad selection. Before final implementation acceptance, obtain the
unchanged full gate on the exact candidate, including required Chromium and
Firefox jobs. No release tag or publication is needed. Do not repeat valid
gates unless relevant code, inputs or assumptions changed.

## 8. Performance measurement and acceptance

### Measurement definition

The maintainer's 15–30 minute goal is a merge-readiness target, not a hard timeout
that cancels valid tests at minute 30. Faster than 15 minutes is acceptable.

For each PR's final reviewed head/base combination, measure from creation of
its first applicable ready-PR workflow to successful `CI ok`. Include queueing,
serial scheduling and any retries for that candidate. Report prior superseded
heads, unsuccessful runs and human review delay separately; do not disguise
their cost as successful test execution or silently discard retry time.

Use a fixed cohort of the next ten consecutive code PRs reaching a terminal
outcome after rollout. Record documentation-only PRs separately so skips do
not improve the code average. For merged/green code PRs report arithmetic mean,
median, maximum and fraction within 30 minutes. List failed, cancelled or closed
without-green PRs explicitly; they are not latency successes. If the cohort
has too few successful runs to support a claim, report insufficient evidence
rather than replacing inconvenient entries.

Report the overall code-PR mean as well as frontend, backend, teletext and
broad-change groups. Broad PRs may take longer individually but remain visible
in the overall mean. Also separate self-hosted and hosted runs; do not use a
warm self-hosted estimate to claim hosted acceptance.

### Time budget for an ordinary narrow PR

These are design budgets, not measured promises:

| Critical-path component | Working budget |
|---|---:|
| Preflight, dispatch and ready-job scheduling | 0–3 min |
| Required builds, including matched web assets when selected | 5–12 min |
| Selected test chain | 5–12 min |
| Artifact/gate overhead outside the above | 0–3 min |

Measure overlap and dependency waits instead of adding every parallel job's
duration. Capture selected domains, reasons, revision, runner pool, build/cache
condition, job/step durations, browser-lock waits and selected test counts.
Use existing Actions summaries and timing artifacts. Add only missing bounded
summary fields; do not introduce a dashboard or external telemetry service.

Accept the performance outcome when the cohort demonstrates an overall mean
of at most 30 minutes without unexplained missing required checks. A narrow
subset meeting the budget is a scoped component result, not proof of the
overall average. The current sample does not establish that this goal is
achievable on every runner or for every change.

If the goal is missed, close verified selection/build components and identify
one measured limiting factor. Do not keep adding optimization packages to this
plan. Tool installation/cache work, additional runner capacity, a crate split
or further test decomposition each needs a separate bounded proposal.

## 9. Rollout, rollback and external prerequisites

Apply the selector and workflow changes through normal reviewed PRs; preserve
required `CI ok` protection and exact-source evidence. The rollout should expose
selected and omitted jobs in Preflight before a reviewer needs to inspect logs.

If an omitted affected test is discovered, restore the smallest affected domain
to conservative selection immediately, add the regression and reverify that
boundary. If the selection or artifact contract cannot be trusted, revert the
selection/build optimization to the previous broad path. Do not disable the
required check or bypass artifact verification to unblock merging.

External prerequisites are finite:

| Missing prerequisite | Closure condition |
|---|---|
| Authorized implementation task | Maintainer requests implementation of this plan or an explicitly bounded subset |
| Suitable runner for required capability evidence | Existing required checks actually pass on a capable permitted runner; an unsupported skip is not proof |
| Post-rollout PR sample | The fixed cohort reaches terminal outcomes and its data is available |
| Expired historical Actions evidence | Preserve the dated baseline as historical; use available current timing evidence and disclose the missing source rather than inventing measurements |

When waiting for runner access or naturally occurring PRs, record exactly what
is missing and stop independent activity once the other required components
are complete. Do not create artificial product PRs, repeat unchanged runs or
provision paid infrastructure to fill the sample.

At implementation handoff, update the existing progress/session records,
reference exact verification and distinguish component completion from speed
acceptance. Clean up task-owned temporary branches, worktrees and analysis files
after safely retaining deliverables and evidence, following AGENTS.md.

## 10. Audited source boundaries for DCI-01

The following inventory was inspected at `ce01d498`. It is input to the
selector implementation, not a claim that the workflow already uses it.
Executable examples live in `scripts/ci-domain-cases.json`; the selector tests
validate that their real files and named specs still exist.

| Owner | Audited source boundary | Selection implications |
|---|---|---|
| Library presentation | `admin/pages/libraries/libraries.module.css` | Library browser family; CSS has no native test target |
| Library mixed page | `admin/pages/libraries/mod.rs` | Resource loads, islands, data transformations and five native tests: retain Rust and library E2E |
| Media presentation wrappers | `admin/pages/media/images.rs`, `songs.rs`, `see_all_lists.rs` | Small component wrappers delegate to `MultiSelectBrowserPage`; narrow eligibility requires their restricted component/import shape to remain valid |
| Teletext mixed draft page | `admin/pages/channels/teletext_drafts.rs` | Preview query parsing, diagnostics, publication forms and nineteen native tests: retain Rust and all affected authoring/editor families |
| Teletext readiness | `admin/pages/channels/teletext_readiness.rs` | Maps server readiness and emits a capacity POST form; keep Rust classification until a narrower boundary is proven |
| Server functions | `admin/server_fns/**` | Rust/server contracts even when called only by one page; teletext functions connect authoring, preview, publication and runtime state |
| Native transport | `transport/iptv/**` | Runtime owner, not shared admin UI; teletext output/retention reaches delivery and recovery plus ordinary playback safety |
| Shared presentation | `admin/layout`, `admin/ui`, `admin/style`, `admin/i18n.rs`, `admin/languages` | Changes can reach every page, including teletext; a CSS-only change still selects browsers without native test execution |
| Shared runtime | `host`, crate roots, schema, application service composition and unknown persistence | Reach is not proven narrow; full required fallback |
| Native FFmpeg audio primitives | `ersatzrs-ffmpeg/src/filter/audio.rs` | Three native `-af` helpers; retain Rust consumers and native preview/playback; other filter modules keep Next until audited |
| Shared FFmpeg configuration | `ersatzrs-ffmpeg/src/options.rs`, runtime/source/track contracts | Next and native consumers may both depend on these types or values; broad playback fallback |

All paths in this table are relative to the named crate's `src` directory
unless the crate name is explicit. Browser scripts under web `public/` are
frontend inputs; `channel-preview.js`, `troubleshooting-preview.js` and add-on
workspace scripts have separate owners. Vendor assets, fonts, icons and
unmapped scripts conservatively affect all browser domains.
`CHANGELOG.md` is embedded by `admin/release_notes.rs` and served at
`/changelog.md`; its content selects `support-pages` and the common smoke set,
not a documentation-only skip. Provider-source changes also select
`teletext-decoder-health`: that spec owns storage Health scenarios despite its
teletext filename. These are explicit dependency edges, not keyword matches.

The twelve currently required Level 1.5 spec files have these family owners.
Overlaps are intentional and preserve whole-file serial/fixture semantics:

- Authoring/publication: `teletext-authoring-flow`,
  `teletext-effective-authoring-flow`, `teletext-guide-authoring-flow`,
  `teletext-channel-profiles`, `teletext-polish-flow`,
  `teletext-definition-removal`.
- Preview/editor: `teletext-editor-flow`, `teletext-authoring-flow`,
  `teletext-channel-profiles`, `teletext-polish-flow`,
  `teletext-definition-removal`.
- Runtime/delivery/recovery: `teletext-restart-flow`,
  `channels-source-teletext-delivery`, `teletext-scripted-health`,
  `teletext-output-policy-health`, `teletext-decoder-health`,
  `teletext-guide-authoring-flow`, `teletext-authoring-flow`,
  `teletext-effective-authoring-flow`, `teletext-channel-profiles`.

The last three runtime overlaps cover actual TCP publication, effective Lua
publication and profile/output assertions. The guide spec covers authoring,
ordinary-stream output, refresh, failures and timeouts, so it must not be split
by test title. `teletext-definition-removal` currently belongs to the base
selection despite its object-related name; preserve the actual suite contract.
The eight enhanced-only filenames remain owned by `ENHANCED_SPECS` in
`ci-teletext-selection.py`, rather than a second independently maintained list.

Unknown imports, new shared modules and ambiguous schema/service reach remain
fallbacks. This inventory does not justify removing tests merely because their
names do not contain the changed feature's keyword.
