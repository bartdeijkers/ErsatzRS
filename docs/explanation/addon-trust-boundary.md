# Why add-on trust is not a sandbox

ErsatzRS add-ons are separately packaged programs that the host starts as
bounded child processes. Provenance, package integrity, and process separation
reduce different risks; none provides operating-system confinement.

## Why trust has three tiers

A mandatory signing key for every publisher would exclude private, offline,
and local-development packages and make operating key infrastructure the entry
price for a third-party repository. ErsatzRS therefore records how each
installation was obtained instead of presenting every source as equally
trusted.

- **Verified** means a repository's identity and Ed25519 public key are pinned.
  The detached signature authenticates exact catalog bytes, which in turn pin
  each package URL, size, and digest.
- **Unverified** means an HTTPS repository has no signing key. Its package
  sizes and digests still protect against accidental corruption, but origin
  authenticity rests on TLS and the repository host.
- **Sideloaded** means the operator supplied a local ZIP with no catalog. The
  host validates and hashes that package, but there is no publisher provenance
  or automatic update source.

Unverified and sideloaded installs are behind one off-by-default **Unknown
sources** gate. The tier remains attached to the installation so the add-on
detail page, source manager, and health report can show the actual support and
risk context later. Disabling the gate prevents new unverified activity; it
does not pretend existing code disappeared.

The exact catalog and package formats are defined by the [repository
contract](../reference/addon-repositories.md) and [package
contract](../reference/addon-packages.md).

## What the process boundary provides

An add-on is not loaded into the ErsatzRS address space through a Rust or native
library ABI. The host selects a package-local entrypoint, clears most inherited
environment variables, supplies bounded operation input, limits structured
output and runtime where the capability permits it, and owns cancellation.

This keeps add-on crashes and protocol failures out of the host process and
allows the contract to evolve independently of Rust ABI details. It does not
change the operating-system identity: the child still runs as the same service
account as ErsatzRS.

## Why permissions remain declarations

Package permissions tell the operator which network hosts, media access,
managed tools, and external commands a package expects. The host uses some of
those declarations when constructing an invocation, but the current contract
does not install a portable operating-system sandbox or firewall policy.

An enabled malicious add-on can therefore attempt anything allowed to the
ErsatzRS service account, regardless of its trust badge. Run ErsatzRS with the
least filesystem and network access it needs, review requested permissions, and
treat a repository key as a publisher identity rather than a safety
certificate.

[ADR 0066](../adr/0066-repository-backed-add-ons.md) owns the tiered-trust and
out-of-process decision. The [runtime contract](../reference/addon-runtime.md)
owns the exact implemented invocation behavior.
