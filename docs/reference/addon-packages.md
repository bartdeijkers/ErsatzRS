# Add-on package reference

An add-on package is a ZIP archive whose root contains `addon.toml`. The
manifest is parsed strictly; unknown fields and invalid cross-field
combinations are rejected.

## Package limits

| Limit | Value |
|---|---:|
| Compressed package | 256 MiB |
| Expanded content | 512 MiB |
| Archive entries | 2,048 |
| `addon.toml` | 256 KiB |
| Optional icon | 2 MiB |

Extraction rejects parent traversal, absolute paths, symlinks, and portable
path collisions. Installation records both the package SHA-256 and a
deterministic hash of the extracted content. The content hash is checked before
every invocation and by health checks.

## Manifest fields

| Field | Contract |
|---|---|
| `manifest_version` | Integer `1` or `2`; version 2 adds the optional `icon` object. |
| `id` | Lowercase reverse-DNS identity with at least three components. |
| `version` | SemVer. |
| `name`, `summary` | Locale maps with a non-empty `en-US` value. |
| `license` | Non-empty SPDX identifier or expression. |
| `source_url` | Credential-free HTTPS URL. |
| `host_version` | SemVer requirement matched against the ErsatzRS version. |
| `icon` | Optional schema-v2 object with a package-relative `path` and `media_type` of `png` or `webp`. |
| `capabilities` | Unique versioned IDs; every executable package includes `addon.check.v1`. |
| `dependencies` | Unique add-on IDs with SemVer requirements and an optional flag. |
| `entrypoints` | Exact release RID keys or `any`, with a package-relative path and launcher kind. |
| `permissions` | Declared network hosts, media-read access, managed FFmpeg access, and external commands. |
| `settings` | Host-rendered definitions with unique uppercase snake-case keys. |
| `panels` | Optional host-rendered declarative blocks. |

Required dependencies must be installed and enabled at a compatible version.
Supported RIDs are `linux-x64`, `linux-arm64`, `linux-arm`, `osx-x64`,
`osx-arm64`, and `win-x64`. Entrypoint kinds are `native`, `posix_shell`, and
`windows_batch`; arbitrary interpreter strings are not part of the current
contract.

An icon path must resolve to a regular file inside the package. ErsatzRS checks
the declared media type against the PNG or WebP file signature and refuses
oversized or mismatched content. Add-ons without an icon use the host-generated
letter placeholder.

## Settings and panels

Setting kinds are `string`, `boolean`, `integer`, `enum`, `path`, `executable`,
and `secret_reference`. Secret definitions cannot have defaults. A secret
setting stores an environment-variable name or file path, never the resolved
secret value.

Declarative panels accept text, status, key/value, and HTTPS link blocks without
URL userinfo. Packages cannot inject HTML, CSS, JavaScript, or WASM into the
admin UI.

## Permission declarations

Permissions are displayed before installation and enablement. They describe
the network hosts, media paths, managed tools, and external executables a
package expects; they are not a portable operating-system sandbox.

The catalog repeats identity, version, display/source metadata, host range,
capabilities, and permissions. Repository installation succeeds only when
those values match the package manifest exactly. Sideload installation has no
catalog cross-check and records the locally supplied archive digest instead.
