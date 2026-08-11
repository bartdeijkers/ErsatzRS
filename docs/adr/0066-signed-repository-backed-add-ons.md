# ADR 0066: Use signed repository-backed out-of-process add-ons

## Status

Accepted (2026-08-11).

## Implementation

Partial — the host platform is implemented in the current source tree,
including conditional signed-catalog refresh, package installation and update,
settings, readiness, declarative panels, health, Remote Stream list/play
dispatch, lifecycle concurrency guards, and managed Unix process-group/Windows
Job Object ownership. The [add-on-platform progress
record](../progress/parity/addon-platform.toml) tracks the remaining external
publication of the official repository and production public-key pin.

## Context

Before this decision, managed Remote Stream playlists selected an
operator-configured “extractor” command, while hand-authored Remote Stream YAML
could execute a raw playback script. Beeld & Geluid and yt-dlp wrappers
consequently shipped inside the application assets even though they had their
own provider dependencies, release cadence, configuration, and platform
behavior.

That model does not provide installation state, updates, package provenance,
capability discovery, or a common settings surface. It also makes arbitrary
command paths part of scanned media configuration. ErsatzTV has no equivalent
repository-backed extension system, so changing this boundary is an intentional
ErsatzRS extension and breaking deviation.

Phase 9 teletext remains independently gated by its product and standards ADR.
The generic add-on boundary must not decide its decoder, schema, or delivery
contract implicitly.

## Decision

ErsatzRS uses versioned, out-of-process add-ons installed from trusted
repositories. It does not load add-on code through a Rust/native dynamic
library ABI.

Each package contains a strict `addon.toml` manifest with a stable reverse-DNS
identifier, SemVer version, supported host range and RIDs, versioned
capabilities, package-local entrypoints, declared permissions, host-rendered
settings, localized metadata, and declarative panels. The shared
`ersatzrs-addon-contract` crate and add-on contract reference own these public
formats.

Repository catalogs are detached Ed25519-signed exact JSON bytes. A trusted
repository record pins its HTTPS index URL and public key independently. The
signed index supplies a monotone sequence, expiry, immutable package URL,
SHA-256, size, compatibility, and display metadata. Packages are downloaded
only after explicit installation, verified before bounded path-safe extraction,
and activated atomically. Catalog refresh is automatic and conditional;
installation and updates are manual.

The official `bartdeijkers/ErsatzRS-addons` repository is the default trusted
source. In ordinary installations it is seeded once and the administrator may
replace it. `ERSATZRS_ADDONS__TRUSTED_REPOSITORIES_FILE` selects an
authoritative deployment-managed repository list; while set, repository
administration is read-only and no implicit official source is added.

Installed add-ons begin disabled. Enable validates required settings,
dependencies, RID/host compatibility, package integrity, and
`addon.check.v1`. Disable prevents new invocations; an owning playback session
retains responsibility for cancelling its already running producer. Updates
stage and validate a newer version before switching new invocations and require
a fresh enable check.
Removing a repository does not remove installed package files or data. Its
foreign-key association is cleared, so the installation no longer has that
repository as a recorded update source. A stable repository-scoped data
namespace remains with the installation, preventing a later package with the
same add-on ID but a different configured repository ID from inheriting private
state. Replacing the URL or key of that same repository identity deliberately
retains its namespace.

Add-on settings support non-secret scalar/path/executable values and secret
references. A secret reference contains only an environment-variable name or
file path; raw secret values are never persisted, displayed, or included in
process arguments. Environment values and operation URLs are exactly redacted
from add-on-controlled output. File-backed references suppress such output
because the host deliberately does not read the secret value needed for exact
redaction. The runner exposes only declared references and retains existing
Remote Stream output limits and cancellation.

The admin UI renders add-on metadata, settings, and declarative panels itself.
Panels may contain only the versioned host-owned component set and cannot render
or execute package-supplied HTML, CSS, JavaScript, or WASM.

Remote Stream playlist manifests and single-stream definitions select an
installed add-on through `addon:`. The old `extractor:` field,
`settings.remote_stream_extractors`, and raw `script:` source are removed
without a compatibility alias. The physical `RemoteStream.Script` column may
remain temporarily as inert migration residue, but application parsing,
scanning, health, probing, and playback reject or report legacy state.

The first executable contracts are `addon.check.v1`,
`remote-stream.list.v1`, and `remote-stream.play.v1`. Unknown capability
versions are never executed. This ADR does not define a teletext capability or
authorize libzvbi packaging; P9A-01 retains ownership of that product,
standards, six-RID runtime, fallback, selection, and output decision.

Signing proves catalog/package provenance and integrity, not confinement.
Add-on processes run as the ErsatzRS operating-system user. Requested
permissions are shown before installation and enablement, and the UI warns
when trusting a non-official repository. The host owns each invocation's Unix
process group or Windows Job Object and terminates every member that remains in
that managed group or job on timeout, cancellation, output-drain failure, or
teardown; this lifecycle boundary is not an operating-system sandbox.

## Consequences

- Provider integrations can ship and update independently without becoming
  application assets or raw commands in scanned YAML.
- Administrators gain an explicit install/configure/enable/update lifecycle and
  deployments can replace the complete trust root.
- Beeld & Geluid retains standalone Windows and Unix implementations; yt-dlp
  remains an operator-supplied external executable.
- Existing extractor settings, raw script definitions, and generated
  definitions require manual YAML migration and rescan.
- The host gains catalog, cryptographic, package, managed process-group/job
  lifecycle, persistence, localization, health, and UI maintenance obligations.
- A malicious but correctly signed trusted add-on has the host user’s access;
  portable OS sandboxing remains future work and must not be implied.
- The official signing private key is operational secret material held only by
  the repository release secret store. Only its public key is committed or
  accepted through configuration.

## Alternatives Considered

- Keep named extractor commands: rejected because it lacks package provenance,
  lifecycle, configuration, and reusable capability discovery.
- Retain raw `script:` as an escape hatch: rejected because it bypasses the
  repository trust boundary and makes the breaking migration ineffective.
- Load Rust or native dynamic libraries: rejected because ABI and process
  isolation would be fragile across the six supported targets.
- Allow add-on JavaScript/WASM: rejected because it bypasses Leptos hydration,
  localization, content-security, and host compatibility contracts.
- Bundle yt-dlp: rejected because its fast release cadence and per-platform
  packaging should remain independently operator-managed.
- Package libzvbi only inside a teletext add-on: deferred to P9A-01; this ADR
  does not decide teletext packaging or the FFmpeg 9 capability matrix.

## References

- [ADR 0051: Application boundary and composition root](0051-application-boundary-and-composition-root.md)
- [ADR 0065: Managed Remote Stream playlists](0065-managed-remote-stream-playlists.md)
- [Add-on contract reference](../reference/addons.md)
- [Remote Stream playlist manifest reference](../reference/remote-stream-playlist-manifests.md)
- [Teletext support plan](../../plans/teletext-support.md)
