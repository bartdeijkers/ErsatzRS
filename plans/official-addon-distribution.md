# Official add-on distribution and Sources design

## Status and ownership

Planned on 2026-10-09; production implementation is deferred until both Phase 9
teletext (`p9b-02`) and Phase 10 video jukebox (`p10b-02`) are Complete.
Milestones `oad-01` through `oad-04` in
[structured progress](../docs/progress/milestones.toml) own acceptance and
evidence. [ADR 0118](../docs/adr/0118-bundle-and-refresh-official-addon-repository.md)
owns the accepted direction. This plan owns implementation sequencing and
unsettled details, not a second acceptance checklist.

The feature is Deferred. Its milestones are Not started with dependency edges
so `cargo xtask progress milestones --runnable` will discover the work after
the prerequisites close. The current focus and Phase 11 numbering are unchanged.

The [HTML/CSS design artifact v0.1](../docs/reference/ui/design-artifacts/addon-sources/LOCK.md)
was explicitly locked by the maintainer on 2026-10-09. Further refinements may
proceed as new revisions while other phases are implemented; preserve v0.1.
Design work does not make an implementation milestone In progress. Do not
publish this behavior as current user documentation before delivery.

## User outcome

An operator starts ErsatzRS with the official repository already available,
receives refreshed official offers without repeated uploads, can disable
automatic repository updates, and can still install an unknown-source add-on
or repository ZIP. Technical Git/storage details do not obstruct those tasks.

## Evidence to revalidate at implementation start

- `crates/ersatzrs-web/src/host/bootstrap.rs` currently scans local repositories
  at startup only when Unknown sources is enabled.
- `crates/ersatzrs-infra/src/addons.rs` owns the local source scan, safe ZIP
  import, revision/fingerprint guards, atomic source replacement and immutable
  installed snapshots. It currently gates bundle import on Unknown sources.
- `crates/ersatzrs-web/src/host/addons.rs` composes the admin runtime; app
  service/port boundaries must continue to own use cases.
- `crates/ersatzrs-web/src/admin/pages/addons.rs` renders an unlabelled checkout
  path as code, a local scan action labelled Refresh, and separate ZIP rows.
  Its source-removal confirmation differs between hydrated and native POST
  paths. Reinspect these findings because intervening phase work may fix them.
- `.github/workflows/release.yml`, `scripts/release/`, and Docker packaging own
  actual distributions. Packaging is undergoing separate maintenance; inspect
  its final native-archive/image flow instead of copying today's staging logic.
- The official repository was public at the planning review. Release v0.11.6
  supplies a repository ZIP and checksum file; this is historical discovery,
  not a selected implementation pin or verified compatibility claim.
- The screenshot/source audit established task friction and source-level
  contrast concerns, not mobile, light-theme or live keyboard acceptance.

## Implementation sequence

| Milestone | Work boundary | Depends on |
|---|---|---|
| `oad-01` | Compatible bundled source, provenance, first-run defaults and existing-profile adoption | `p9b-02`, `p10b-02` |
| `oad-02` | Persisted automatic repository update policy and bounded refresh | `oad-01` |
| `oad-03` | Implement locked Sources v0.1, localized UI and preserved ZIP/Git workflows | `oad-02` |
| `oad-04` | Integrated packages, migration, failure recovery and preservation proof | `oad-03` |

### OAD-01: Bundle and initialize

1. Revalidate public distribution, release assets, manifest host-version ranges,
   per-RID entrypoints and external tool requirements at the completed Phase 10
   baseline. Select a compatible stable bundle by exact version and digest.
2. Stage the same tested repository snapshot and notices into every supported
   native package and relevant Docker image. Keep the add-ons repository
   independent; do not vendor its mutable working tree into runtime execution.
3. Introduce an application-owned official-source identity/provenance path using
   existing validation and storage. Uploaded content cannot grant itself trust.
4. Initialize through profile-owned writable storage. Distinguish a new profile
   from an existing profile with newly absent fields. Seed the official source
   enabled and automatic updates On once, retaining later choices and contents.
5. Specify existing-profile adoption before migration code: no ID collision,
   replacement of operator checkouts, settings loss, downgrade or silent trust
   promotion. Keep explicit source removal/disablement durable across upgrades.

The current install paths intentionally refuse source changes and some duplicate
identities. Extend them deliberately for reviewed official adoption; do not
bypass these checks by rewriting IDs or toggling the global trust gate.

### OAD-02: Refresh published repository contents

Use the published repository bundle and the smallest suitable existing HTTP,
validation and background-work capabilities. A remote Git checkout, private
credentials, general plugin marketplace and generic updater framework are out
of scope. Arbitrary third-party online repositories are not required here.

Proposed cadence: a due check after startup and once per day, with bounded
backoff and remote Retry-After handling. Confirm concrete cadence, download
limits and timeout values against the current infrastructure before coding.
Only published compatible stable releases are candidates by default.

Persist source policy, last attempt, last successful check, installed repository
identity and enough due/retry state for restart safety. Reuse existing bounded
workers and admission controls; serialize automatic, manual and import/adoption
operations for a source. Use validation and revision fencing before atomic
promotion; cancellation or process death must leave the last complete source
usable and allow bounded cleanup/recovery. Disabling automatic updates prevents
new scheduled work and fences a pending promotion; define cancellation behavior
for an already admitted manual check separately.

Keep source/catalog refresh, installed add-on upgrades, and native yt-dlp tool
updates separate. The toggle changes only repository refresh. Define an allowed
official origin/redirect policy and integrity/provenance verification; a checksum
downloaded beside an archive detects corruption but is not an independent
publisher signature. Do not label an unsigned source cryptographically verified.

Expose useful source errors and stale-check state through the existing Health
engine. A temporary network failure must not imply installed add-ons are broken,
block startup or interrupt playback. Automatic Off is intentional configuration,
not a warning by itself.

### OAD-03: Sources administration

Use the locked v0.1 artifact before production UI work. Reconcile it with any
shared-shell, component or localization changes delivered in Phases 9 and 10.

The design contract includes:

- An official-source region with visible availability, bundled/downloaded
  version, automatic-update toggle, last successful check and manual action.
- A visible Install from file region for both add-on and repository ZIPs, with
  local trust controls, contextual validation, pending/error/success states and
  a direct next action after a repository import.
- A subordinate manual Git region: labelled read-only server checkout path,
  copy affordance, clone/pull guidance and an accurately named Rescan action.
- Explicit source kind and freshness; zero known updates is not proof that an
  old offline bundle is current. Expand ambiguous enabled/installed counts.
- Remove source wording that explains installed packages/settings survive,
  with equivalent confirmation and cancellation in hydrated and no-JS flows.
- Readable helper text, visible focus, persistent labels, reachable actions,
  responsive reflow and every supported culture. Keep controls and announced
  states stable across hydration.

Locked v0.1 selects a single ZIP picker with validated package-type detection,
not a new manifest format or a reason to remove support for either ZIP kind.
Changing that interaction requires a new explicitly accepted design revision.
Add-on IDs, local paths and hashes belong in supporting details unless needed
to resolve a concrete conflict.

### OAD-04: Integrated acceptance

Execute the canonical milestone acceptance against exact package and bundle
identities. Reuse evidence that remains valid. Inspect all supported package
layouts and run platform-appropriate first-run/upgrade checks; record missing
native evidence explicitly rather than borrowing unrelated phase exceptions.
Test Docker with fresh/persistent profiles and read-only installation files.

Validate offline bootstrap, preserved opt-out after restart/application upgrade,
manual refresh, fault/cancellation recovery, source adoption and both ZIP kinds.
Prove a repository refresh leaves running package bytes, enablement, settings,
saved lists, publication and playback unchanged. Select ordinary smoke and any
playback-sensitive gates from the actual diff; do not waive the repository's
final closeout or packaged acceptance contracts.

## Design iteration before implementation

The artifact README owns preview commands, review prompts and revision procedure;
LOCK.md pins the accepted v0.1 reference. Create further design drafts in a new
sibling artifact directory instead of changing locked files. Change
the ADR/milestone agreement only for an explicitly accepted scope decision.
Use an isolated worktree for simultaneous design and runtime coding, and merge
only the reviewed design paths; neither design review nor locking authorizes
starting the deferred implementation or publishing a release.

## Open decisions and exclusions

- No affirmative choice to preinstall/enable yt-dlp was recorded. The accepted
  baseline enables the repository only. Revisit executable defaults only if
  the maintainer explicitly requests them; package readiness is separate.
- Exact cadence, stable release selection, provenance transport and initial
  migration UX require bounded implementation decisions in OAD-01/OAD-02.
- General third-party online catalogs, signing-key infrastructure, new native
  tool maintenance, automatic executable upgrades and a shell redesign are not
  part of this plan.
- Design draft approval, implementation completion, exact-SHA CI, native package
  acceptance and release publication remain distinct gates.

## Resume commands

```bash
cargo xtask progress milestones --id p9b-02 --full
cargo xtask progress milestones --id p10b-02 --full
cargo xtask progress milestones --feature official-addon-distribution
cargo xtask progress parity --feature official-addon-distribution --full
cargo xtask progress milestones --runnable
```

After both phase gates close, start at OAD-01, reread the current source map,
ADR and artifact revision, then update the feature from Deferred to In progress.
