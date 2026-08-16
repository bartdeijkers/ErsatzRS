# Add-on package reference

An add-on package is a ZIP archive whose root contains `addon.toml`. The
manifest is parsed strictly; unknown fields and invalid cross-field
combinations are rejected.

## Rust contract crate

The distribution mechanism for Rust add-ons is the independently versioned
`ersatzrs-addon-contract` crate in the canonical ErsatzRS GitHub repository.
Contract version `0.1.1` is the current source version. Once its implementation
is committed, it receives the immutable `addon-contract-v0.1.1` tag. Add-ons
pin the tag's full commit SHA:

```toml
[dependencies.ersatzrs-addon-contract]
git = "https://github.com/bartdeijkers/ErsatzRS"
rev = "<contract-commit-sha>"
```

The `rev` value is the 40-character commit named by the contract tag. The crate
declares `publish = false` and is not distributed through crates.io or another
registry. Its source package, tests, and rustdoc are validated before the tag is
pushed. ErsatzRS tags a compatible contract version before the corresponding
official add-on change is published. Add-ons update the revision and lockfile
deliberately; they do not depend on a moving branch or the application
workspace by path.

`AddonMediaListRecord` owns `media-list.list.v1` and v2 records.
`PlaylistEntry` owns `remote-stream.list.v1` and v2 records. Their shared wire
enums are distinct from the host's persisted domain enums. The crate also
exports `conform_media_list_output` and
`conform_remote_stream_list_output` for tests that validate captured stdout
with the same bounds and record rules as the host contract. A minimal executable
implementation is available as the `reference_addon` Cargo example.

## Standalone contract validation

Validate package manifests, repository-package manifests, and generated
repository catalogs through the same Rust types and `validate()` methods used
by the host:

```bash
cargo xtask validate-addon-manifest path/to/addon.toml
cargo xtask validate-addon-manifest path/to/repository.toml
cargo xtask validate-addon-manifest --kind catalog path/to/index-v1.json
```

The dependency-light `ersatzrs-addon-validator` binary exposes the same checks
for pinned cross-repository CI use. Publication-specific checks such as package
directory naming, icon bytes, artifact presence, ZIP construction, and catalog
assembly remain the publisher's responsibility; they are not a second manifest
contract.

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
| `manifest_version` | Integer `1`, `2`, or `3`; version 2 adds the optional `icon` object and version 3 adds safe suggested secret-reference locations and managed media-list storage. |
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
| `media_list_storage` | Optional schema-v3 link between a path setting, a safe profile-relative default folder, and an add-on-owned local media source. |
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
secret value. A schema-v3 secret definition may declare
`suggested_reference = { kind = "environment" | "file", reference = "..." }`.
This prefills only the reference location in the host-rendered editor and is
not a secret default; non-secret settings cannot declare it.

Declarative panels accept text, status, key/value, and HTTPS link blocks without
URL userinfo. Packages cannot inject HTML, CSS, JavaScript, or WASM into the
admin UI.

## Permission declarations

Permissions are displayed before installation and enablement. They describe
the network hosts, media paths, managed tools, and external executables a
package expects; they are not a portable operating-system sandbox.

`network_hosts` also constrains the one operator-supplied source URL that the
host passes to a `media-list.list` or `remote-stream.list` operation. An empty
list or `*` accepts every host. A plain host such as `media.example.test`
matches only that exact host. `.example.test` and `*.example.test` match real
subdomains such as `media.example.test`, but not the apex `example.test` and
not lookalikes such as `badexample.test`. Host matching is ASCII
case-insensitive. Operator source URLs may use HTTP or HTTPS, including local
and on-premise HTTP services, but must be absolute, bounded, and contain no
username or password.
The host passes the parser's canonical URL serialization to the add-on rather
than the original input, preventing the host and add-on from interpreting
different authorities around parser-sensitive characters.

This check validates a host-controlled input before invocation; it does not
confine the child process or prevent it from contacting other destinations.
See [ADR 0066](../adr/0066-repository-backed-add-ons.md#what-trust-does-not-mean)
for the runtime trust boundary.

The catalog repeats identity, version, display/source metadata, host range,
capabilities, and permissions. Repository installation succeeds only when
those values match the package manifest exactly. Sideload installation has no
catalog cross-check and records the locally supplied archive digest instead.
