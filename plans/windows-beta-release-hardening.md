# Windows beta release hardening

## Status

Actionable. This plan owns the remaining Windows release hardening and the
acceptance work for the next immutable beta after `v1.0.21-beta.1`. It does not
reopen the published tag or replace historical release evidence.

Current release behavior is defined by
[`release.yml`](../.github/workflows/release.yml), the operator procedure lives
in [`publish-release.md`](../docs/how-to/publish-release.md), and the exact
archive contract belongs to
[`release-bundle.md`](../docs/reference/release-bundle.md). Keep facts in those
owners and use this document only for future work and acceptance sequencing.

## Goal

Produce and validate the exact Windows candidate archive before publication,
then publish a beta only after local gates, private live playback acceptance,
exact-SHA CI, and independent archive verification all agree.

## Release-blocker boundary

The draft-first candidate path, candidate/beta bundle-consistency gate, and
the mandatory acceptance section are blockers for the next beta. Action pinning,
policy decisions and dependency hygiene remain important
maintainer debt but do not independently block a beta whose exact candidate
passes the mandatory gates. The maintainer withdrew Beeld & Geluid support
and its acceptance on 2026-09-11 under ADR 0092; do not access that service.

## Workstream A: draft-first candidate archive

The maintainer approved draft-first staging on 2026-09-11, superseding the
separate manually dispatched candidate path. Require exact-SHA green CI, then
push a new immutable beta tag. The existing Windows beta job builds once,
checks the web bundle and archive contents, and stages an unpublished draft
prerelease. Download and accept that ZIP, then publish the same draft without
rebuilding or replacing its asset. The tag is visible before manual acceptance;
a failed candidate consumes its beta version. Never move or reuse the tag.

The beta job must refuse to overwrite published releases and stop on API or
upload failure. Draft replacements invalidate prior archive acceptance; do not
publish while a workflow rerun is active. The canonical command sequence and
digest checks live in
[the publication guide](../docs/how-to/publish-release.md#accept-and-publish-a-windows-beta-draft).

## Workstream B: harden the publishing workflow

- Retain the structural web-bundle consistency gate already present in the
  Windows beta job before draft creation. Static file-presence checks alone do
  not prove that the SSR executable and hydration bundle belong to one build.
- Pin third-party reusable actions to reviewed full commit SHAs, prioritizing
  jobs with `contents: write`, `packages: write`, or OIDC permissions. Retain a
  readable version comment beside each pin and define an update cadence.
- Decide whether Windows beta binaries participate in the optional signing
  path. Either sign them through the same reviewed implementation as normal
  Windows archives or document beta archives as intentionally unsigned.
- Keep the tag-triggered beta job non-publishing until all validation steps
  succeed. A failed candidate must never leave a partial release that looks
  complete.

## Workstream C: settle release-policy details

Record each decision in its canonical documentation instead of only checking a
box here:

- The maintainer approved the tag/candidate date for draft-first betas;
  [RELEASING.md](../RELEASING.md#release-checklist) owns that convention.
- Decide whether GitHub release bodies must reproduce the curated changelog
  summary or may use generated notes. Align `RELEASING.md` with the chosen
  source of truth.
- Keep beta signing expectations consistent between `RELEASING.md`, the
  workflow, and the published release.
- Track the transitive `proc-macro-error2` future-incompatibility through its
  owning Leptos dependency and remove stale `deny.toml` skips in a separate
  dependency-hygiene slice. Do not add an ad hoc release-only patch.

## Workstream D: extractor and operator preflight

- Beeld & Geluid parser work is withdrawn under ADR 0092, not deferred.
- Reconcile any configured finite-source duration with the current provider
  manifest. Record an intentional trim explicitly; otherwise correct the
  private definition before candidate acceptance.
- Treat FFmpeg profile display names as labels, not codec evidence. Clone the
  persisted profile values for a disposable acceptance channel and resolve any
  stale operator-facing name separately.

Provider URLs, signed parameters, private media names, database rows, and
maintainer-machine paths must remain outside committed files and logs.

For native Windows acceptance, set every isolation variable inside the Windows
shell that launches the server. Reject the run unless the first bootstrap line
identifies the disposable database. Hash the server executable actually
launched and compare it with the executable extracted from the candidate
archive before treating any playback result as release evidence.

## Mandatory acceptance for the next beta

First run the repository closeout gates, push the exact candidate SHA, and
require terminal successful CI for that SHA. Then create and push a new
annotated beta tag and wait for the Windows draft build. Download its ZIP into
a fresh temporary directory; verify its digest, ZIP integrity, four executables,
web assets, and excluded build/private trees. Verify the separately installed
provider package and its entrypoints as required below. Check that the tag,
workflow, local commit and live remote ref identify the intended candidate.

Run these gates against that independently downloaded draft archive and its
managed Windows FFmpeg runtime (retaining valid evidence where applicable):

The maintainer withdrew the manual anamorphic regression-source check on
2026-09-11; it is not passed and no fixture/profile locations are required.
The existing
`nvenc_pipeline_anamorphic_source_scales_directly_to_square_pixels_snapshot`
in `crates/ersatzrs-ffmpeg/tests/pipeline_snapshots.rs` remains command-level
coverage, not proof of decoded native GPU output.

1. Exercise a finite YouTube definition near its midpoint through the installed
   yt-dlp add-on. Prove producer-side seek, prompt valid output, quiet success,
   a concise failing diagnostic, and candidate/deployed provider-package and
   entrypoint identity. Accepted ADR 0066 removed manual extractors and bundled
   wrappers; this uses their supported replacement without waiving any of the
   playback, diagnostic or identity requirements.
2. Re-run direct IPTV and segmenter HLS smoke after the temporary live setup.
3. Complete the current milestone's integrated Windows acceptance, including
   native add-on process containment tests for the candidate's dependencies.
4. Re-download the draft asset and require its digest to equal the accepted
   ZIP. With no release workflow running, publish that same draft; do not
   rebuild or upload another asset.
5. Independently download the published ZIP, require the accepted digest, and
   confirm prerelease status and the intended immutable tag/source identity.

## Completion

This plan can be pruned when:

- the draft-first candidate path and beta web-bundle gate are implemented;
- release policy and signing expectations are documented;
- the remaining dependency-hygiene items are closed or moved to narrower
  canonical owners;
- every mandatory live gate passes against one exact candidate; and
- the resulting beta archive has independent publication evidence.

The next beta may proceed once the explicit release blockers pass even if a
non-blocking item remains. Move any such residue to a narrower canonical owner
before pruning this plan.
