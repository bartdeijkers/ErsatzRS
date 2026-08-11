# Why signed add-ons are not a sandbox

ErsatzRS add-ons are separately packaged programs that the host starts as
bounded child processes. Signing and process separation reduce several risks,
but they answer different questions from operating-system confinement.

## What repository trust proves

A trusted repository is pinned by identity, HTTPS location, and an Ed25519
public key. Its detached signature authenticates the exact catalog bytes. The
catalog then pins each package URL, size, and SHA-256 digest, while installation
records a second digest of the extracted files.

Together, those checks establish provenance and detect a catalog rollback,
package substitution, or later package modification. They do not establish
that the signed program is benign. Trusting a repository means trusting its
publisher to choose and build the code that will run.

The exact catalog and package formats are defined by the
[repository contract](../reference/addon-repositories.md) and
[package contract](../reference/addon-packages.md).

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
those declarations when constructing an invocation, but v1 does not install a
portable operating-system sandbox or firewall policy.

An enabled malicious add-on can therefore attempt anything allowed to the
ErsatzRS service account. Run ErsatzRS with the least filesystem and network
access it needs, review third-party source and requested permissions, and treat
a repository public key as a trust root rather than a safety certificate.

[ADR 0066](../adr/0066-signed-repository-backed-add-ons.md) owns the decision to
use signed repositories and out-of-process execution. The
[runtime contract](../reference/addon-runtime.md) owns the exact implemented
invocation behavior.
