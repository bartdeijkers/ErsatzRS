# Manage add-ons

Add-ons extend ErsatzRS through signed packages and bounded child processes.
This guide applies to a development build that exposes **Settings > Add-ons**;
the [platform reference](../reference/addons.md) records current availability.
Use the workflow for Remote Stream providers and future capabilities, and do
not put executable commands in media YAML.

## Install from a trusted repository

1. Open **Settings > Add-ons**.
2. Refresh a trusted repository.
3. Review the source, version, capabilities, dependencies, media access,
   network hosts, managed FFmpeg access, and external commands shown for the
   package.
4. Select **Install**. A new package starts disabled.
5. Configure every required setting.
6. Select **Check** and resolve any unavailable dependency.
7. Select **Enable**.

The official repository entry is seeded in a new database. A source build or
unpublished deployment may intentionally contain no official public key yet;
such an entry cannot refresh until the release maintainer pins the repository's
raw Ed25519 public key. Never bypass that check with an unsigned catalog.

Updates are manual. The available list shows **Update** when the signed catalog
offers a different version. ErsatzRS rejects SemVer downgrades, stages and
verifies the new immutable package, switches the active record, and leaves it
disabled until **Check** succeeds again.

## Configure secrets by reference

A normal setting is stored in the ErsatzRS database. A secret setting stores
only one of these references:

- **Environment variable**: an uppercase environment-variable name already
  available to the ErsatzRS service.
- **Secret file**: an absolute path readable by the ErsatzRS service account.

An environment secret is resolved only while the invocation environment is
built. A secret-file reference remains a path that the add-on reads itself.
Raw secret values are not displayed, persisted, placed in process arguments,
or included in diagnostics. Configure the environment variable or mounted file
outside the admin UI.

## Add a custom repository

Select **Add repository** and enter a stable reverse-DNS ID, display name,
credential-free HTTPS index URL, and the repository's raw 32-byte Ed25519
public key encoded as standard Base64. A repository signature establishes
provenance; it does not make third-party code safe. Review its source and
permissions before installation.

Removing a repository does not remove packages already installed from it.

## Replace the complete allowlist for a deployment

Create the authoritative TOML file defined by the
[repository reference](../reference/addon-repositories.md), then set
`ERSATZRS_ADDONS__TRUSTED_REPOSITORIES_FILE` to its path. Prefer an absolute
path for a service so its working directory cannot change the result. Restart
ErsatzRS and confirm that the repository editor is read-only and that only the
file's entries are listed.

Keep the private signing key only in the repository publisher's secret store;
ErsatzRS needs the public key only.

## Disable or uninstall

**Disable** blocks new invocations. A stream already owned by a playback
session is stopped by that session's normal cancellation lifecycle.

**Disable** and **Uninstall** are also blocked while another enabled or installed
package has a required dependency on the add-on. Remove that dependent package
or migrate it first. **Uninstall** remains blocked while a Remote Stream refers
to the add-on. Persistent add-on data is retained so reinstalling can recover
operator state; package executables are removed.

Use **Support > Health Checks** to find expired catalogs, failed refreshes,
modified package content, incompatible manifests, and enabled packages without
a readiness result.
