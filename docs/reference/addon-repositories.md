# Add-on repository reference

ErsatzRS discovers add-on repositories from operator-managed local Git
checkouts or from complete offline ZIP bundles. It never invokes Git, receives
Git credentials, downloads a catalog, or updates a checkout.

The off-by-default **Unknown sources** setting gates repository scanning,
repository-bundle upload, and installation from every local source. Installed
add-ons continue to run when that setting is turned off.

## Repository layout

A repository uses one strict, self-contained layout:

```text
repository.toml
addons/
  org.example.addon.one/
    addon.toml
    ...package files...
  org.example.addon.two/
    addon.toml
    ...package files...
```

Each direct child of `addons/` must be named exactly like the `id` in its
`addon.toml`. Symlinks are refused. A repository scan validates every manifest,
package-local icon, and entrypoint for the current release RID when one is
declared. An add-on without an entrypoint for the current RID remains visible
as unavailable rather than invalidating a cross-platform repository.

## `repository.toml`

The root manifest is strict TOML:

```toml
schema_version = 1
id = "org.example.ersatzrs.addons"
name = "Example add-ons"

[description]
"en-US" = "Example local add-on repository"
```

`schema_version`, `id`, and `name` are required. `id` is a stable lowercase
reverse-DNS identity. `description` is optional; when present it requires a
non-empty `en-US` value. URL, credential, public-key, signature, and package
index fields are not part of this contract.

Validate the root manifest with the host-owned contract validator:

```bash
cargo xtask validate-addon-manifest path/to/repository.toml
```

## Manual Git checkout

Settings > Add-ons > Sources shows the fixed checkout directory for the active
ErsatzRS profile. Clone each repository as one direct child of that directory,
using normal operator tooling and credentials. Then enable Unknown sources and
choose **Refresh**. ErsatzRS also performs this scan during startup when the
gate is enabled.

A failed repository scan records a health warning and preserves the previous
complete offer set. Removing a checkout withdraws its offers and marks its
source missing; already installed immutable snapshots are not deleted. Pulling
new commits changes no running package until the operator refreshes and
explicitly updates that add-on.

## Offline repository bundle

A repository bundle is a ZIP whose root contains the complete layout above.
Uploading it creates or replaces one managed local source and lists its offers;
it does not install every add-on. Replacement is atomic at the persisted source
boundary, and the superseded extracted bundle is removed after a successful
scan.

| Limit | Value |
|---|---:|
| Compressed repository ZIP | 512 MiB |
| Expanded content | 512 MiB |
| Archive entries | 2,048 |
| `repository.toml` | 256 KiB |

Extraction rejects parent traversal, absolute paths, symlinks, special files,
and portable case-colliding paths. A checkout and an offline bundle cannot own
the same repository ID at the same time.

## Installation and updates

Scanning records immutable offer fingerprints but does not execute repository
content. Installing one offer verifies that its files still match the scanned
fingerprint, creates a bounded ZIP internally, extracts it into the managed
package store, and records both archive and content hashes. Runtime invocation
uses only that immutable installed snapshot.

Updates are manual per add-on. ErsatzRS refuses source changes, downgrades, and
same-version replacement. The only same-version exception is an explicit
adoption of a legacy catalog installation; that transition preserves settings
and the add-on data namespace while replacing its provenance with the selected
local source.

Installations left behind by the former HTTPS catalog model remain runnable as
**Legacy catalog · unmanaged**. They have no automatic update source and are
reported as such by add-on health until adopted or uninstalled.
