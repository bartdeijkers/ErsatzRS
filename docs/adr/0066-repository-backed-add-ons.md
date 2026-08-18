# ADR 0066: Use repository-backed out-of-process add-ons with tiered trust

## Status

Accepted (2026-08-12; drafted 2026-08-11, reworked before acceptance to adopt
Kodi-shaped distribution, and amended 2026-08-15 to keep the Rust contract
crate GitHub-only).

## Implementation

Partial. The full host and management contract is implemented in the current
source tree, including
signed-catalog refresh, package installation and update, settings, readiness,
declarative panels, health, Remote Stream list/play dispatch, lifecycle
concurrency guards, and managed Unix process-group/Windows Job Object
ownership. The tiered trust model, repository packages, package sideloading,
off-by-default unknown-sources gate, persistent trust tiers, health warnings,
optional schema-v2 icons, mature manager views, and per-add-on detail routes
were implemented on 2026-08-12. External publication and production-key
provisioning for the official repository remain tracked separately in the
[add-on-platform progress record](../progress/parity/addon-platform.toml).

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

The original 2026-08-11 decision required a detached Ed25519 signature on every
repository catalog, with no other way to install a package. Two problems
surfaced before the first publication:

1. **No distribution path without a public host.** A catalog must be fetched
   anonymously — the client rejects URLs carrying userinfo and sends no
   credentials — so the artifacts must be publicly readable. Combined with
   HTTPS-only fetching against the bundled Mozilla root set, there was no way to
   install anything from a private source, a local build, or an offline medium.
2. **A signing pipeline as the cost of entry.** Requiring every repository to
   pin a key means every third-party author must run Ed25519 key custody. The
   comparable projects do not: Kodi publishes `addons.xml` with only an MD5
   change-detection digest and no signature at all, and Stash's
   `CommunityScripts` index carries per-package SHA-256 with no signature.
   Both rely on HTTPS, curation, and an explicit "unknown sources" opt-in, and
   both have third-party ecosystems that a mandatory-signing contract would
   have prevented.

Signing remains the strongest available provenance guarantee and is worth
keeping where it is affordable. It is not worth making it the only door.

Phase 9 teletext remains independently gated by its product and standards ADR.
The generic add-on boundary must not decide its decoder, schema, or delivery
contract implicitly.

## Decision

ErsatzRS uses versioned, out-of-process add-ons installed from repositories or
from local packages. It does not load add-on code through a Rust/native dynamic
library ABI.

Each package contains a strict `addon.toml` manifest with a stable reverse-DNS
identifier, SemVer version, supported host range and RIDs, optional bounded
PNG/WebP artwork, versioned
capabilities, package-local entrypoints, declared permissions, host-rendered
settings, localized metadata, and declarative panels. The shared
`ersatzrs-addon-contract` crate and add-on contract reference own these public
formats.

### Rust contract distribution

The Rust contract crate is distributed only from the canonical
`bartdeijkers/ErsatzRS` GitHub repository. Each contract release has an
immutable `addon-contract-v<version>` tag, while official Rust add-ons pin the
tag's full commit SHA through Cargo's `git` and `rev` dependency fields. The
human-readable tag records the contract version; the SHA is the reproducible
build identity and remains authoritative if a tag is ever moved accidentally.

The crate retains its own SemVer package version but declares
`publish = false`; it is never uploaded to crates.io or another package
registry. A correction is released as a new version and tag rather than by
moving an existing tag. Host changes land and receive their immutable tag
before an official add-on updates its pinned revision and lockfile.

### Tiered trust

Every installation records a permanent trust tier. The tier is a visible
property of what is installed, never a precondition for installing at all.

**Verified.** The repository record pins a raw Ed25519 public key. Its catalog
is exact JSON bytes authenticated by a detached signature at `<index>.sig`,
fetched conditionally with `ETag`/`Last-Modified` validators, and revalidated
against previously signature-verified bytes on `304`. Rollback and expiry
protection are cryptographically meaningful because a monotone sequence and
validity interval are covered by the signature.

**Unverified.** The repository record pins no key. Its catalog is fetched over
HTTPS from a publicly trusted certificate authority with no signature. Schema
v1 conformance, repository-identity match, monotone sequence, expiry, HTTPS
package URLs without userinfo, and per-package SHA-256 and size remain
mandatory. The sequence and expiry are honest-error protection here, not
security controls: an attacker who controls the origin controls those values
too. Authenticity rests entirely on TLS and on the origin not being
compromised.

**Sideloaded.** A package installed from a local file, with no repository and
no catalog. The recorded SHA-256 is computed from the supplied bytes at install
time. Manifest schema validation, `manifest.validate()`, RID and host-range
compatibility, entrypoint presence, bounded path-safe extraction, and content
hashing are all still mandatory. Only catalog cross-checks are absent.

Verified installations are always permitted. Unverified repositories and
sideloaded packages both require the administrator to enable unverified
sources, a single setting that is **off by default** and presents the trust
consequences before it can be turned on. While it is off, an unverified
repository cannot be added or refreshed and no local package can be installed.
This mirrors Kodi's "Unknown sources" gate.

### Repository packages

A repository is distributed as a small zip containing a `repository.toml`
manifest declaring a reverse-DNS identifier, display name, HTTPS index URL, and
optionally a raw Ed25519 public key. Installing that package creates or updates
the trust record; the presence of a key decides the resulting tier. This is the
mechanism that makes a third-party ecosystem practical: one file to distribute,
with no form-filling and no manually pasted key.

A repository package is itself unsigned, so installing one is always gated by
the unverified-sources setting even when the manifest pins a key. What a pinned
key buys is that every subsequent catalog fetch is signature-verified against
it — trust on first use, then cryptographic continuity, in the same shape as an
SSH host key. The administrator remains free to add a repository through the
admin form instead.

The official `bartdeijkers/ErsatzRS-addons` repository is the exception. Its
identity, URL, and public key are seeded from the ErsatzRS source tree, so it is
a compile-time trust root rather than a trust-on-first-use decision, and it
needs no unverified-sources opt-in.

`ERSATZRS_ADDONS__TRUSTED_REPOSITORIES_FILE` continues to select an
authoritative deployment-managed repository list. While it is set, repository
administration is read-only, repository packages cannot be installed, and no
implicit official source is added. Its `public_key` field becomes optional and
decides that entry's tier. The file governs repositories only; sideloading
remains governed by the unverified-sources setting, so a locked-down deployment
sets the file and leaves that setting off.

### Lifecycle

Packages are downloaded or supplied only after explicit installation, verified
before bounded path-safe extraction, and activated atomically. Installed
add-ons begin disabled. Enable validates required settings, dependencies,
RID/host compatibility, package integrity, and `addon.check.v1`. Disable
prevents new invocations; an owning playback session retains responsibility for
cancelling its already running producer. Updates stage and validate a newer
version before switching new invocations and require a fresh enable check.

Catalog refresh is automatic and conditional. **Installation and updates stay
manual**, deliberately diverging from Kodi, which updates add-ons automatically
by default. Add-on processes here run with the host user's access and declared
permissions, so silently replacing their code — particularly from an unverified
origin — is not an acceptable default. Automatic updates restricted to verified
repositories are possible future work.

Version selection also stays explicit: an add-on is installed from a chosen
source, downgrades are refused, and a package from a different source cannot
displace an existing installation without an uninstall. Kodi instead resolves
the highest compatible version across all enabled repositories; that behavior is
rejected here because it lets any added repository take ownership of an already
installed identifier.

Removing a repository does not remove installed package files or data. Its
foreign-key association is cleared, so the installation no longer has that
repository as a recorded update source; a sideloaded add-on never has one and
therefore never has an update path, which the UI must state rather than leave
as an apparent defect. A stable data namespace remains with the installation,
scoped to the repository identity that installed it or to a reserved
`sideload` namespace, preventing a later package with the same add-on ID but a
different origin from inheriting private state. Replacing the URL or key of that
same repository identity deliberately retains its namespace.

### Settings, panels, and dispatch

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

### What trust does not mean

No tier provides confinement. Add-on processes run as the ErsatzRS
operating-system user. Requested permissions are shown before installation and
enablement, and the UI shows the trust tier wherever an add-on or repository
appears. The host owns each invocation's Unix process group or Windows Job
Object and terminates every member that remains in that managed group or job on
timeout, cancellation, output-drain failure, or teardown; this lifecycle
boundary is not an operating-system sandbox. A malicious add-on has the host
user's access whether it was signed, unsigned, or sideloaded.

## Consequences

- Provider integrations can ship and update independently without becoming
  application assets or raw commands in scanned YAML.
- A third-party author can publish by serving two static files over HTTPS, or
  distribute a single zip, without operating any key infrastructure. This is the
  change that makes an ecosystem plausible.
- A maintainer can keep an add-on source private and still install from a local
  build, and add-on development gains a working local loop.
- Administrators gain an explicit install/configure/enable/update lifecycle and
  deployments can replace the complete trust root.
- The security posture is now a spectrum the administrator chooses, not a
  constant. Leaving unverified sources off preserves exactly the previous
  guarantees; turning it on accepts TLS-and-origin trust for the sources it
  admits. That choice must be legible in the UI and in the health report.
- The health check gains an obligation to report enabled unverified and
  sideloaded add-ons, so the report keeps reflecting real trust posture.
- Every "add-on is broken" support report now has an unverified branch; the
  recorded tier is what makes that diagnosable.
- Beeld & Geluid retains standalone Windows and Unix implementations; yt-dlp
  remains an operator-supplied external executable.
- Existing extractor settings, raw script definitions, and generated
  definitions require manual YAML migration and rescan.
- The host gains catalog, cryptographic, package, managed process-group/job
  lifecycle, persistence, localization, health, and UI maintenance obligations.
- The official signing private key remains operational secret material held only
  by the repository release secret store. Only its public key is committed or
  accepted through configuration.
- Rust add-on builds require Git access to the canonical ErsatzRS repository on
  their first dependency fetch. Their committed Cargo lockfile and exact
  revision prevent a moving branch or tag from changing the selected contract.
- Contract API documentation remains in the repository and generated rustdoc;
  no crates.io or docs.rs discovery surface exists.

## Alternatives Considered

- **Require a pinned key on every repository** (the original 2026-08-11
  decision): rejected because it admits no private, local, or offline
  installation and taxes every third-party author with key custody, for a
  guarantee stronger than either comparable project provides.
- **Drop signing entirely and match Kodi exactly**: rejected because the
  verification path, its tests, and the official repository's signing pipeline
  already exist and work. Deleting a working provenance guarantee to reduce a
  one-time setup cost trades a durable property for a transient convenience.
- **Two separate settings for unverified repositories and sideloading**:
  rejected as a distinction without a difference — both admit code whose
  authenticity rests on the operator's judgment, and Kodi's single gate is
  well understood.
- **Trust a repository package's pinned key without the unverified-sources
  gate**: rejected because the zip carrying the key is itself unauthenticated,
  so honoring the key without an opt-in would let an unsigned artifact silently
  install a trust root.
- **Automatic updates by default, as Kodi does**: rejected for now because
  add-on processes hold the host user's access; revisit for verified
  repositories only.
- **Highest-compatible-version resolution across repositories, as Kodi does**:
  rejected because it lets any added repository take ownership of an already
  installed add-on identifier.
- **Publish the Rust contract crate to crates.io**: rejected because the
  project requires contract publication to remain inside its own GitHub
  repositories. Registry discovery is not worth introducing another public
  distribution channel and credential boundary.
- **Depend on the moving `main` branch or only on a Git tag**: rejected because
  neither declaration makes the reviewed commit identity explicit in the
  consuming manifest. Official Rust add-ons pin the full revision and use the
  immutable tag only as its human-readable release name.
- Keep named extractor commands: rejected because it lacks package provenance,
  lifecycle, configuration, and reusable capability discovery.
- Retain raw `script:` as an escape hatch: rejected because it bypasses the
  repository boundary and makes the breaking migration ineffective.
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
- [Add-on package reference](../reference/addon-packages.md)
- [Add-on repository reference](../reference/addon-repositories.md)
- [Add-on trust boundary](../explanation/addon-trust-boundary.md)
- [Remote Stream playlist manifest reference](../reference/remote-stream-playlist-manifests.md)
- [Add-on platform progress](../progress/parity/addon-platform.toml)
- [Teletext support plan](../../plans/teletext-support.md)
