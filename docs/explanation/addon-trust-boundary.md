# Why add-on trust is not a sandbox

ErsatzRS add-ons are separately packaged programs that the host starts as
bounded child processes. Provenance, package integrity, and process separation
reduce different risks; none provides operating-system confinement.

## Why source provenance remains visible

ErsatzRS records how each installation was obtained instead of presenting
every local file as equally supported.

- **Local** means an operator-managed checkout or uploaded offline repository
  supplied a validated offer. The installed runtime is an immutable copy, not
  the mutable checkout.
- **Sideloaded** means the operator supplied an individual add-on ZIP. The host
  validates and hashes it, but it has no repository update source.
- **Legacy catalog · unmanaged** means the package came from the retired HTTPS
  catalog model. It remains runnable but has no active update source until the
  operator explicitly adopts a local offer.

Local repository scans, bundles, and sideloads are behind one off-by-default
**Unknown sources** gate. Provenance remains attached to the installation so
the detail page, source manager, and health report can show its support and
risk context later. Disabling the gate prevents new local-source activity; it
does not pretend existing code disappeared.

The exact local repository and package formats are defined by the [repository
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
treat source provenance as an audit aid rather than a safety certificate.

[ADR 0066](../adr/0066-repository-backed-add-ons.md) owns the out-of-process
decision; [ADR 0081](../adr/0081-use-local-addon-repositories.md) owns local
acquisition and legacy adoption. The [runtime
contract](../reference/addon-runtime.md) owns the exact implemented invocation
behavior.
