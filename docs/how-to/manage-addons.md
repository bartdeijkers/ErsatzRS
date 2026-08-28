# Install and manage add-ons

Use **Settings > Add-ons** to install, inspect, configure, update, disable, and
remove out-of-process add-ons. The manager separates **Installed**, **Browse**,
**Updates**, and **Sources** so package lifecycle and repository trust remain
visible. Each tab carries its own count; **Installed add-ons** carries two,
written as enabled over installed. Each card opens a dedicated detail route at
`/settings/addons/<add-on-id>`.

**Sources** holds three sections: **Unknown sources** (the risk opt-in),
**Add-on repositories** (local Git checkouts and uploaded offline bundles,
listed with their offer count and last scan), and **Install from file**
(individual add-on ZIPs and complete repository ZIPs). The page also shows the
fixed checkout directory. The **Default remote stream processor** sits on
**Installed add-ons**, with the add-ons it chooses between.

The [platform reference](../reference/addons.md) lists the supported
capabilities. Do not put executable commands in media YAML.

## Install from a repository

1. Open **Settings > Add-ons > Sources** and enable **Unknown sources**.
2. Clone a repository as one direct child of the displayed checkout directory,
   or upload its complete repository ZIP under **Install from file**.
3. Select **Refresh** after cloning or pulling a checkout.
4. Open **Browse**, search for the add-on, and select its card.
5. On the detail page, review its source, trust tier, version, capabilities,
   dependencies, network hosts, media access, managed FFmpeg access, and
   external commands.
6. Select **Install** and confirm. A new package starts disabled.
7. Configure every required setting on the same detail page.
8. Select **Check** and resolve any unavailable dependency.
9. Select **Enable**.

The task is complete when the detail page shows a successful readiness result
and **Enabled** state. **Check** validates readiness but never enables a package.

Updates stay manual. Open **Updates**, select an add-on, review the offered
source and permissions, and select **Update**. ErsatzRS refuses downgrades, and
stages and validates the package.

An add-on that was enabled is checked and enabled again as part of the update,
so a working add-on keeps working. It stays disabled in two cases: the readiness
check fails, which the detail page then explains, or the new version declares a
permission the previous one did not — a new network host, a new external
command, media read access, or managed FFmpeg access. Review the permissions on
the detail page and select **Check** and **Enable** to accept them.

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

## Maintain a local repository

ErsatzRS does not run Git. Pull a checkout with the same operator tooling and
credentials used to clone it, then select **Refresh**. A successful scan
replaces the complete offer set; a failed scan preserves the last valid offers
and adds a health warning. Removing the checkout withdraws its offers without
deleting immutable installed packages.

An offline repository ZIP must contain root `repository.toml` plus the complete
`addons/<id>/` tree described by the [repository
reference](../reference/addon-repositories.md). Uploading a newer bundle
replaces that source, but each add-on update still requires confirmation.

Former catalog installations are labelled **Legacy catalog · unmanaged**. If
a local repository offers the same add-on, its update dialog explicitly asks
to adopt that source. Adoption preserves settings and persistent data.

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

## Disable or uninstall

**Disable** blocks new invocations. A stream already owned by a playback
session is stopped by that session's normal cancellation lifecycle.

Disabling is blocked while another enabled package requires the add-on.
Uninstalling is blocked while any installed package or Remote Stream still
requires it. Persistent add-on data is retained in its origin-scoped namespace;
package executables and disposable cache are removed.

Use **Support > Health Checks** to find failed local-source scans, missing
checkouts, modified package content, incompatible manifests, locally sourced
enabled packages, legacy unmanaged installations, and installed packages
without a readiness result.
