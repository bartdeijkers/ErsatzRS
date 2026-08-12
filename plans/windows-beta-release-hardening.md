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

The non-publishing candidate path, candidate/beta bundle-consistency gate, and
the mandatory acceptance section are blockers for the next beta. Action pinning,
policy decisions, parser hardening, and dependency hygiene remain important
maintainer debt but do not independently block a beta whose exact candidate
passes the mandatory gates.

## Workstream A: non-publishing candidate archive

The current Windows beta archive is built only after an immutable beta tag is
pushed. Add a manually dispatched candidate path that:

1. accepts an explicit commit SHA and checks out exactly that revision;
2. runs on the Windows release runner without creating or moving a tag;
3. uses the same build, staging, and validation implementation as the beta
   release job rather than maintaining a second packaging recipe;
4. uploads a short-lived artifact whose name and metadata identify the full
   source SHA;
5. requires only read/artifact permissions and cannot create a GitHub release;
6. proves the four executables and non-empty CSS, JavaScript, and WebAssembly
   assets are present; and
7. runs `scripts/release/verify-web-bundle.sh` against the assembled candidate.

Acceptance requires the downloaded candidate archive to reproduce the workflow
SHA and pass the same integrity and content checks used after publication.

## Workstream B: harden the publishing workflow

- Run the structural web-bundle consistency gate in the Windows beta job before
  `gh release create`. Static file-presence checks do not prove that the SSR
  executable and hydration bundle belong to one build.
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

- Choose the changelog date convention for releases whose UTC and maintainer
  local dates differ.
- Decide whether GitHub release bodies must reproduce the curated changelog
  summary or may use generated notes. Align `RELEASING.md` with the chosen
  source of truth.
- Keep beta signing expectations consistent between `RELEASING.md`, the
  workflow, and the published release.
- Track the transitive `proc-macro-error2` future-incompatibility through its
  owning Leptos dependency and remove stale `deny.toml` skips in a separate
  dependency-hygiene slice. Do not add an ad hoc release-only patch.

## Workstream D: extractor and operator preflight

- Make the Windows Beeld & Geluid RSC parser byte-accurate. RSC chunk lengths
  are byte counts, so scan the raw response bytes and UTF-8-decode only the
  selected chunk. Add offline fixtures with non-ASCII text before and inside a
  signed-URL chunk.
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

Run these gates against the independently downloaded candidate archive and its
managed Windows FFmpeg runtime:

1. Exercise the shipped Beeld & Geluid wrapper from source start and a
   substantial producer-side seek. Require prompt, sustained, decodable audio
   and video and byte-for-byte wrapper/template identity with the candidate.
   Confirm that the configured duration either matches the current manifest or
   records an intentional trim.
2. Stream the maintainer-provided anamorphic regression source through a
   disposable channel using a clone of the real persisted profile values.
   Check start, midpoint tune, segment transition, restart, decoded colours,
   audio/video continuity, fallback behavior, and bounded supervisor retries.
3. Exercise a finite YouTube definition near its midpoint with the optional
   manual extractor. Prove producer-side seek, prompt valid output, quiet
   success, a concise failing diagnostic, and candidate/deployed wrapper
   identity.
4. Re-run direct IPTV and segmenter HLS smoke after the temporary live setup.
5. Run the repository closeout gates and push the exact candidate SHA. Require
   terminal CI success for that SHA.
6. Build or select the Windows archive for that same SHA, download it into a
   fresh temporary directory, and verify its digest, ZIP integrity, four
   executables, web assets, extractor assets, and excluded build/private trees.
7. Only then create and push a new annotated beta tag. Confirm the tag, release
   workflow, release asset, local commit, tracking ref, and live remote ref all
   identify the intended immutable candidate.

## Completion

This plan can be pruned when:

- the non-publishing candidate path and beta web-bundle gate are implemented;
- release policy and signing expectations are documented;
- the parser and dependency-hygiene items are closed or moved to narrower
  canonical owners;
- every mandatory live gate passes against one exact candidate; and
- the resulting beta archive has independent publication evidence.

The next beta may proceed once the explicit release blockers pass even if a
non-blocking item remains. Move any such residue to a narrower canonical owner
before pruning this plan.
