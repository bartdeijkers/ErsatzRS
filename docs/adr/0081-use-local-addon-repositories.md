# ADR 0081: Use local add-on repositories and offline bundles

## Status

Accepted (2026-08-28)

## Implementation

Implemented (2026-08-28)

[ADR 0118](0118-bundle-and-refresh-official-addon-repository.md) accepts a future
replacement of the official source's local-only acquisition policy after Phase 9
and Phase 10. It is not implemented yet. The manual checkout/ZIP and immutable
execution contracts below remain in effect; the new feature owns future debt.

## Context

ADR 0066 introduced public HTTPS catalogs, detached signatures, repository
packages, and local add-on ZIPs. The official add-on repository is now private
to avoid automated contribution spam. ErsatzRS must not receive Git credentials
or require a public catalog merely to install those add-ons.

The existing out-of-process manifest, permissions, settings, readiness,
immutable package, and process-lifecycle boundaries remain valuable. Only the
acquisition and update source needs to change.

## Decision

Operators acquire repositories themselves, normally with `git clone` or
`git pull`, below the host-reported add-on checkout directory. ErsatzRS never
invokes Git. It scans a strict `repository.toml` and `addons/<id>/addon.toml`
layout and presents validated add-ons as offers.

An offline repository ZIP contains that same complete layout. Uploading it
creates or atomically replaces a managed local source; it does not install all
contained add-ons. An individual add-on ZIP remains a direct sideload.

Installation and updates are explicit per add-on. The host copies selected
content into its immutable package store and runs only that snapshot. A later
checkout edit has no effect until a rescan and confirmed update. The existing
off-by-default Unknown Sources setting gates every local source and ZIP path.

Remote catalog downloads, signatures, public keys, index expiry, conditional
HTTP refresh, the seeded official repository, and deployment repository
allowlists are removed from the active runtime. Existing catalog-installed
packages remain operational as legacy unmanaged installations until the
operator explicitly adopts a local offer.

## Consequences

- Private Git authentication stays entirely in operator tooling.
- Checkout and offline ZIP workflows share one manifest and validation path.
- Add-ons keep immutable runtime contents, bounded execution, settings, and
  update permission checks.
- Updates require `git pull` plus Rescan, or upload of a newer bundle, followed
  by an explicit per-add-on update.
- Historical catalog tables remain as compatibility residue while referenced;
  no runtime code refreshes or installs from them.

## Alternatives Considered

- Let ErsatzRS clone and pull: rejected because it would own Git processes and
  credentials.
- Execute directly from checkouts: rejected because mutable working trees
  would bypass installed-content integrity.
- Keep private HTTPS catalogs with credentials: rejected because it recreates
  secret distribution and unattended network-fetch responsibilities.

## References

- [ADR 0066](0066-repository-backed-add-ons.md)
- [Add-on package reference](../reference/addon-packages.md)
- [Add-on repository reference](../reference/addon-repositories.md)
