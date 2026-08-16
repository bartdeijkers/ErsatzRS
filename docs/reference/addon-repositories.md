# Add-on repository reference

An add-on repository has a stable reverse-DNS ID and a credential-free HTTPS
index URL. A repository with a pinned Ed25519 public key is **verified**; one
without a key is **unverified** and requires the off-by-default
`addons.allow_unverified_sources` setting.

The optional public key is canonical standard Base64 for a raw 32-byte Ed25519
key. When present, it is exactly 44 ASCII characters, contains no whitespace,
decodes to 32 bytes, and re-encodes exactly.

## Deployment allowlist

`ERSATZRS_ADDONS__TRUSTED_REPOSITORIES_FILE` replaces the complete
database-backed repository list. Its value is read as a filesystem path; a
relative path is resolved from the ErsatzRS process working directory.

The file is UTF-8 TOML, is limited to 256 KiB, and contains zero or more unique
repository IDs and URLs:

```toml
[[repositories]]
id = "org.example.ersatzrs.addons"
name = "Example add-ons"
index_url = "https://addons.example/index-v1.json"
public_key = "<optional-base64-encoded-raw-ed25519-public-key>"
official = false
enabled = true
```

`public_key` is optional. `official` defaults to `false` and `enabled` defaults
to `true`. Use `repositories = []` to configure no repositories. While the
setting is present, database repository editing and repository-package upload
are disabled. Package sideloading remains controlled separately by the
unverified-sources setting.

## Repository packages

A repository package is a ZIP of at most 256 KiB whose root contains a strict
`repository.toml`:

```toml
id = "org.example.ersatzrs.addons"
name = "Example add-ons"
index_url = "https://addons.example/index-v1.json"
public_key = "<optional-base64-encoded-raw-ed25519-public-key>"

[description]
"en-US" = "Example repository"
```

`id`, `name`, and `index_url` are required. `public_key` and the localized
`description` map are optional; a non-empty description requires `en-US`.
Installing a repository package always requires unverified sources because the
ZIP carrying the initial trust record is not authenticated. It cannot replace
the seeded official repository or a deployment-managed list.

## Catalog index

The catalog body is limited to 4 MiB. The strict index object contains:

| Field | Contract |
|---|---|
| `schema_version` | Integer `1` or `2`; version 2 adds catalog icon metadata. |
| `repository_id` | Must equal the configured repository identity. |
| `repository_name` | Non-empty display name. |
| `sequence` | Monotone unsigned rollback-detection value. |
| `generated_at`, `expires_at` | RFC 3339 validity interval. |
| `addons` | Unique catalog entries. |

Each catalog entry supplies identity, version, localized metadata, license,
source URL, host requirement, capabilities, permissions, and packages. Its
`packages` map selects an exact RID before `any`; each package supplies an HTTPS
URL without userinfo, query, or fragment, plus lowercase SHA-256 and byte size.
Redirect targets remain HTTPS.

Schema 2 may add an `icon` object with an HTTPS URL, `png` or `webp` media type,
lowercase SHA-256, and a byte size from 1 byte through 2 MiB. ErsatzRS verifies
the response size, digest, final HTTPS URL, and file signature before caching or
serving it.

For a verified repository, ErsatzRS downloads a Base64 detached signature from
`<index-url>.sig`, limited to 1 KiB, and authenticates the exact catalog bytes
before JSON decoding. For an unverified repository it skips this signature
request. Schema, identity, sequence, expiry, HTTPS, package digest, and package
size validation apply to both tiers. Sequence and expiry detect honest errors
for an unverified origin; they are not authenticity controls.

Reusing a sequence with different bytes is rejected. Reusing it with identical
bytes is accepted. Expired catalogs remain stored for diagnosis but cannot
install or update packages.

The shared contract crate validates the context-independent catalog structure:
schema bounds, repository identity syntax and display name, RFC 3339 interval,
unique add-on IDs, catalog entries, icons, package URLs, digests, and package
size. The host then applies trust-record context such as exact repository
identity, monotone sequence, current-time expiry, and signature verification.

## Refresh and removal

The web host checks repositories when its maintenance task starts and every six
hours. A repository is refreshed when its previous successful refresh is at
least twelve hours old. ErsatzRS persists `ETag` and `Last-Modified` validators
and sends them on the next request. A valid `304 Not Modified` response reuses,
reparses, and revalidates the matching cached bytes. Verified cached bytes also
retain their signature-verification state. A trust-root change invalidates the
cached revision.

A new verified `200 OK` response always fetches and verifies the detached
signature before replacing the cache. A new unverified response is validated
without one. Compare-and-swap revisions prevent a stale refresh from
overwriting a changed trust root or newer catalog sequence. Failed refreshes
preserve the last valid catalog.

The official repository never falls back to the unverified tier. Until its
production public key is pinned, refresh fails closed even when unverified
sources are enabled.

Removing a database-backed repository does not uninstall its packages. The
installation keeps its recorded trust tier, but its repository association is
unset and it no longer has a catalog-driven update source.
