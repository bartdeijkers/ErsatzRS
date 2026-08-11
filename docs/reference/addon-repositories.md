# Add-on repository reference

An add-on repository is trusted only after ErsatzRS has an allowlist record
containing its stable ID, credential-free HTTPS index URL, and raw Ed25519
public key in canonical standard Base64. The key must be exactly 44 ASCII
characters, contain no whitespace, decode to 32 bytes, and re-encode exactly.

## Deployment allowlist

`ERSATZRS_ADDONS__TRUSTED_REPOSITORIES_FILE` replaces the complete
database-backed allowlist. Its value is read as a filesystem path; a relative
path is resolved from the ErsatzRS process working directory. Use an absolute
path for service deployments so startup does not depend on that directory.

The file is UTF-8 TOML, is limited to 256 KiB, and contains zero or more unique
repository IDs and URLs:

```toml
[[repositories]]
id = "org.example.ersatzrs.addons"
name = "Example add-ons"
index_url = "https://addons.example/index-v1.json"
public_key = "<base64-encoded-raw-ed25519-public-key>"
official = false
enabled = true
```

`official` defaults to `false` and `enabled` defaults to `true`. Use
`repositories = []` to trust no repository. While the setting is present,
database-only repositories cannot refresh or install packages and the admin
repository editor is read-only.

## Signed index

ErsatzRS downloads exact `index-v1.json` bytes and a Base64 detached signature
from `index-v1.json.sig`. The index is limited to 4 MiB and the signature to
1 KiB. Signature verification happens before JSON decoding.

The strict index object contains:

| Field | Contract |
|---|---|
| `schema_version` | Integer `1`. |
| `repository_id` | Must equal the pinned allowlist identity. |
| `repository_name` | Non-empty display name. |
| `sequence` | Monotone unsigned rollback-prevention value. |
| `generated_at`, `expires_at` | RFC 3339 validity interval. |
| `addons` | Unique catalog entries. |

Each catalog entry supplies identity, version, localized metadata, license,
source URL, host requirement, capabilities, permissions, and packages. Its
`packages` map selects an exact RID before `any`; each package supplies an HTTPS
URL without userinfo, query, or fragment, plus SHA-256 and byte size. Redirect
targets must remain HTTPS and may use provider-owned query parameters.

Reusing a sequence with different signed bytes is rejected. Reusing it with
identical bytes is accepted. Expired catalogs remain stored for diagnosis but
cannot install or update packages.

## Refresh and removal

The web host checks repositories when its maintenance task starts and every six
hours. A repository is refreshed when its previous successful refresh is at
least twelve hours old. ErsatzRS persists the signed index response's `ETag` and
`Last-Modified` values and sends them on the next index request. A valid `304
Not Modified` response reuses, reparses, and revalidates the exact previously
signature-verified catalog bytes without requesting the detached signature.
It is rejected when no matching validator or verified catalog is available.

A `200 OK` response always fetches the detached signature and verifies the new
exact bytes before replacing the cache. Compare-and-swap repository revisions
prevent a stale refresh from overwriting a changed trust root or rolling the
catalog sequence backward. A failed or concurrent refresh preserves the last
verified catalog.

Removing a database-backed repository does not uninstall its packages. The
installation's repository association becomes unset, so it no longer has that
source for catalog-driven updates.

The source tree seeds an official repository row with an empty public key.
Until a release pins the production public key, that row cannot refresh; a
custom repository with a valid pinned key remains usable.
