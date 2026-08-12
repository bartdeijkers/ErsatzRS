# Install and manage add-ons

Use **Settings > Add-ons** to install, inspect, configure, update, disable, and
remove out-of-process add-ons. The manager separates **Installed**, **Browse**,
**Updates**, and **Sources** so package lifecycle and repository trust remain
visible. Each card opens a dedicated detail route at
`/settings/addons/<add-on-id>`.

The [platform reference](../reference/addons.md) lists the supported
capabilities. Do not put executable commands in media YAML.

## Install from a repository

1. Open **Settings > Add-ons > Sources** and refresh the intended repository.
2. Open **Browse**, search for the add-on, and select its card.
3. On the detail page, review its source, trust tier, version, capabilities,
   dependencies, network hosts, media access, managed FFmpeg access, and
   external commands.
4. Select **Install** and confirm. A new package starts disabled.
5. Configure every required setting on the same detail page.
6. Select **Check** and resolve any unavailable dependency.
7. Select **Enable**.

The task is complete when the detail page shows a successful readiness result
and **Enabled** state. **Check** validates readiness but never enables a package.

Updates stay manual. Open **Updates**, select an add-on, review the offered
source and permissions, and select **Update**. ErsatzRS refuses downgrades,
stages and validates the package, and leaves the updated add-on disabled. Run
**Check** and **Enable** again.

## Install a local package

Local packages and unsigned repository sources require an explicit risk opt-in:

1. Open **Settings > Add-ons > Sources**.
2. Under **Unknown sources**, select **Enable** and confirm the warning.
3. Choose an add-on ZIP under **Install from file**, then select **Install**.
4. Open the installed add-on's detail page. Confirm that its trust badge says
   **Sideloaded**, review its permissions, configure it, run **Check**, and
   enable it only when ready.

A sideloaded package has no repository update source. Install a newer local ZIP
to update it. A sideload cannot replace a repository-backed package with the
same ID; uninstall the existing package first. Disabling unknown sources later
blocks new unverified installs and refreshes but does not remove existing
packages.

The package layout and limits are defined by the [package
reference](../reference/addon-packages.md).

## Add an unverified repository

After enabling unknown sources, use either workflow:

- Select **Add repository** and enter its reverse-DNS ID, display name,
  credential-free HTTPS index URL, and optional raw Ed25519 public key.
- Choose a repository ZIP and select **Install**. Its root must contain the
  strict `repository.toml` described by the [repository
  reference](../reference/addon-repositories.md).

A source with a public key is **Verified** after the initial trust record is
accepted; subsequent catalogs must carry a valid detached signature. A source
without a key is **Unverified** and relies on HTTPS plus catalog package
digests. Review the source before refreshing or installing from it.

Removing a repository does not remove packages already installed from it.

## Configure secrets by reference

A normal setting is stored in the ErsatzRS database. A secret setting stores
only one of these references:

- **Environment variable**: an uppercase environment-variable name already
  available to the ErsatzRS service.
- **Secret file**: an absolute path readable by the ErsatzRS service account.

Raw secret values are not displayed, persisted, placed in process arguments,
or included in diagnostics. Configure the environment variable or mounted file
outside the admin UI. Clear an optional secret setting by saving it with no
reference; this also disables the add-on until it passes a fresh check and is
enabled again.

## Replace repository administration for a deployment

Create the authoritative TOML file defined by the [repository
reference](../reference/addon-repositories.md), set
`ERSATZRS_ADDONS__TRUSTED_REPOSITORIES_FILE` to its path, and restart ErsatzRS.
Confirm that **Sources** is read-only and lists only the file's entries.

Keep private signing keys only in repository publishers' secret stores;
ErsatzRS needs public keys only.

## Disable or uninstall

**Disable** blocks new invocations. A stream already owned by a playback
session is stopped by that session's normal cancellation lifecycle.

Disabling is blocked while another enabled package requires the add-on.
Uninstalling is blocked while any installed package or Remote Stream still
requires it. Persistent add-on data is retained in its origin-scoped namespace;
package executables and disposable cache are removed.

Use **Support > Health Checks** to find expired catalogs, failed refreshes,
modified package content, incompatible manifests, unverified enabled packages,
and installed packages without a readiness result.
