# Scope: tiered trust, repository packages, and sideload installation

Status: **implemented 2026-08-12**
Discovered: 2026-08-12
Implements: [ADR 0066](../docs/adr/0066-repository-backed-add-ons.md) as reworked 2026-08-12
Tracked in: `docs/progress/parity/addon-platform.toml`

## Delivery outcome

The planned validation split, persistent trust tiers, unverified repository
refresh, official-source fail-closed guard, add-on and repository ZIP installs,
unknown-sources setting, application boundary, multipart API, localized admin
manager, health reporting, and focused regression coverage are implemented.
The management surface now includes Installed, Browse, Updates, and Sources
views plus `/settings/addons/{addon_id}` detail pages. Manifest and catalog
schema 2 add optional bounded PNG/WebP artwork while schema 1 remains readable.

The work breakdown below is retained as the delivery rationale and audit trail;
its future-tense wording describes the pre-implementation plan. Publication of
the separate official repository and provisioning its production signing key
remain external release work, not implementation debt in this plan.

## Problem

ErsatzRS can only install an add-on that appears in a refreshed, Ed25519-signed
repository catalog. There is no equivalent of Kodi's "Install from zip file",
and no way to run a repository without key infrastructure. Confirmed by search:
no sideload, `from_zip`, `install_from`, or `file://` path exists in
`crates/ersatzrs-infra/src/addons.rs` or the admin server functions.

This blocks three real cases:

1. Publishing an official catalog requires a public HTTPS host. A maintainer who
   wants to keep the add-on source private has no way to install anything.
2. Add-on development has no local loop. `validate_https_url`
   (`addons.rs:2982`) rejects everything but `https://`; loopback `http://` is
   allowed only under `cfg!(test)` (`:2985`). All three crates pin `reqwest`
   with `rustls-tls`, and `Cargo.lock` resolves `webpki-roots` with no
   `rustls-native-certs` — so the add-on client trusts **only** the bundled
   Mozilla root set and an mkcert/self-signed local cert will be rejected.
3. Third-party authors must stand up an Ed25519 signing pipeline to distribute
   anything. Kodi and Stash both have ecosystems partly because their barrier is
   a zip plus a manifest.

## Decided model

Settled 2026-08-12 and recorded in
[ADR 0066](../docs/adr/0066-repository-backed-add-ons.md), which was reworked in
place (it is still `draft`, so no superseding document was needed):

- **Tiered trust.** Verified (repository pins an Ed25519 key, catalog
  signature-checked as today), unverified (repository pins no key, HTTPS +
  per-package SHA-256), sideloaded (local file, no catalog). The tier is
  recorded permanently per installation and shown wherever the add-on appears.
- **One gate.** A single admin setting, off by default, admits both unverified
  repositories and sideloaded packages — Kodi's "Unknown sources" shape.
- **Repository packages.** A repository is distributable as a zip containing a
  `repository.toml` manifest. Installing it creates the trust record;
  a pinned key in that manifest yields the verified tier on a
  trust-on-first-use basis. Always gated by the unverified-sources setting,
  because the zip itself is unauthenticated.
- **Nothing is deleted.** `ed25519-dalek`, `decode_public_key`,
  `verify_detached_signature`, and the official repository's signing pipeline
  all stay.
- **Updates stay manual** and version selection stays explicit, deliberately
  diverging from Kodi on both.

## What already works and is reusable

The install pipeline splits cleanly. From `install_catalog_package`
(`addons.rs:726`):

| Stage | Function | Reusable? |
| --- | --- | --- |
| Fetch bytes | `download_catalog_package:759` | No — replaced by upload for sideload |
| Extract safely | `extract_zip_safely` via `stage_catalog_package_inner:811` | Yes, unchanged |
| Validate package | `validate_extracted_package:960` | **Partly — needs a split** |
| Hash content | `hash_addon_tree:1869` | Yes, unchanged |
| Place on disk | `stage_catalog_package_inner:831-859` | Yes, minor change |
| Persist row | `persist_staged_addon:862` | No — needs a sibling |

Lifecycle after install (`check_addon`, `enable_addon`, `disable_addon`,
`uninstall_addon`, settings, dependency guards, managed process ownership) is
already origin-agnostic and needs no change. `AddonInstallation.RepositoryId` is
already `NULL`-able with `ON DELETE SET NULL`
(`migrations/202608111200_addon_platform.sql:23`), so the schema already
tolerates an installation with no repository.

## Work breakdown

### 1. Split package validation — `crates/ersatzrs-infra/src/addons.rs`

`validate_extracted_package:960` currently does two distinct jobs. Split it:

- `validate_extracted_manifest(extracted_root, rid) -> AddonManifest` — lines
  965–981 and 1010–1018: bounded `addon.toml`, schema v1 parse,
  `manifest.validate()`, entrypoint exists for this RID. Used by every path.
- `ensure_manifest_matches_catalog(&manifest, addon)` — lines 982–1009: identity,
  name, summary, license, source URL, host range, dependencies, permissions, and
  capability-set equality with the catalog entry. Catalog paths only.

This is the change that unlocks everything else. It is mechanical and should be
landed as its own commit with the existing tests still green.

`stage_catalog_package` / `stage_catalog_package_inner:783,811` take
`&CatalogAddon` purely to read `addon.id` and `addon.version` for the
destination path (`:831`). Change them to take the validated manifest's id and
version instead, so one staging function serves all paths.

### 2. Trust tier — migration and model

New migration pair, `migrations/<ts>_addon_trust_tier.sql` and the `mysql/`
counterpart, adding
`AddonInstallation."TrustTier" TEXT NOT NULL DEFAULT 'verified'`
with values `verified` | `unverified` | `sideloaded`.

`RepositoryId IS NULL` is **not** a usable proxy for sideloaded: the FK is
`ON DELETE SET NULL`, so a catalog install whose repository was later removed
also has a null `RepositoryId` and would be misreported.

The repository tier needs no column — it is derived from `PublicKey` being
empty, which the existing schema already permits.

Do not edit the applied migrations; sqlx checksums them.

### 3. Unverified repository tier — `addons.rs`

- `validate_repository_entry:2185` currently hard-rejects an empty public key
  (`:2205`). Make that conditional: empty is allowed only when unverified
  sources are enabled, and `decode_public_key` still runs whenever a key *is*
  present.
- `refresh_repository_inner:533` currently rejects an empty key outright
  (`:538`). Branch instead: with a key, the existing signature path; without
  one, skip the `.sig` fetch and `verify_detached_signature` and go straight to
  schema/identity/sequence/expiry validation. Everything after
  `serde_json::from_str` at `:582` is already tier-independent.
- **Guard the official repository.** `IsOfficial = 1` with an empty key must
  keep failing to refresh rather than silently becoming an unverified source
  when the setting is turned on. Today the seeded official row has
  `PublicKey = ''` (`202608111200_addon_platform.sql:64`), so without this guard
  the rework would quietly grant it the unverified tier.
- The 304 revalidation path must not reuse bytes across a tier change; the
  existing trust-change comparison at `:442-449` already covers a key edit and
  should be extended to cover the tier flip.

### 4. Sideload install entry point — `addons.rs`

New `pub async fn install_addon_from_package(pool, archive: Vec<u8>) -> Result<InstalledAddon, AddonError>`:

- Reject if unverified sources are disabled — enforce in infra, not only in UI.
- Enforce `MAX_PACKAGE_BYTES` (`:56`, 256 MiB) on the uploaded bytes.
- `package_sha256 = sha256_hex(&archive)`, computed from what was supplied,
  since there is no catalog hash to compare against.
- Stage and validate via the split functions from §1.
- Reject if an installation with that `AddonId` exists **and** has a non-null
  `RepositoryId` — a sideload must not displace a catalog-backed install.
  Require explicit uninstall, matching the cross-repository guard at `:689`.
- Allow sideload-over-sideload as an update, reusing the downgrade guard shape
  at `:700`.

### 5. Sideload persistence — `addons.rs`

New `persist_sideloaded_addon`, a sibling of `persist_staged_addon:862`. It
cannot be a parameter tweak: the existing statements bind `repository_id` in
five places and gate on
`EXISTS (SELECT 1 FROM "AddonRepository" WHERE "Id" = ? AND "CatalogRevision" = ? AND "IsEnabled" = 1)`.

- `RepositoryId` = `NULL`, `TrustTier` = `sideloaded`, and the catalog-revision
  `EXISTS` guard dropped.
- Keep `lock_addon_lifecycle`, the enabled-dependents check, the `StateRevision`
  compare-and-swap, and `Enabled = 0` on install.
- `DataNamespace` = reserved constant `sideload`. The catalog path binds it to
  `repository_id` (`:906`, `:940`) so that a same-ID package from a different
  origin cannot inherit private state; the reserved value preserves that
  guarantee for sideloads. ADR 0066 now records this.

### 6. Repository packages — `addons.rs` + `ersatzrs-addon-contract`

- New `RepositoryManifest` in `ersatzrs-addon-contract`: `id` (reverse-DNS,
  validated by the existing `validate_addon_id`), `name`, `index_url`,
  optional `public_key`, optional localized description. Strict
  `deny_unknown_fields`, matching `TrustedRepositoryEntry:279`.
- New `install_repository_from_package(pool, archive)`: bounded extraction into
  a temp staging root, read and validate `repository.toml`, run the existing
  `validate_repository_entry`, then `upsert_repository_row:2214`.
- Always gated by the unverified-sources setting, and always refused while
  `repositories_are_deployment_managed():337` is true.
- The package is small; cap it far below `MAX_PACKAGE_BYTES` (256 KiB is
  consistent with the deployment-file limit).
- Refuse to overwrite an existing repository whose `IsOfficial = 1`.

### 7. Setting + app boundary

- Admin setting `addons.allow_unverified_sources`, default false, stored like
  other admin settings, with a confirmation surface stating the consequences.
- `AddonRuntime` (`crates/ersatzrs-app/src/ports.rs:520`) gains
  `install_from_package(archive: Vec<u8>)` and
  `install_repository_from_package(archive: Vec<u8>)`, implemented in
  `crates/ersatzrs-web/src/host/addons.rs`.
- `AddonOverviewRead` gains `unverified_sources_enabled: bool`, a per-repository
  tier, and a per-installation `TrustTier`, so the UI renders trust state
  without a second round trip.

### 8. Upload transport

**Do not push package bytes through a Leptos server function.** Follow the
established precedent: a plain axum multipart route.

- `crates/ersatzrs-web/src/api/addons.rs`, modelled on `api/watermarks.rs:36`
  (`upload_watermark_image`) — `State(AppState)` + `Multipart`, single field,
  size-checked, JSON error bodies localized through `load_error_language`.
  Two endpoints: add-on package and repository package.
- Register in `host/router.rs` next to `/api/watermarks/image` (`:106`).
- Client side: `web_sys::FormData` + `Request::post`, exactly as
  `admin/pages/watermarks/editor.rs:1010-1031`.
- Enforce the size cap at the axum layer too, so an oversized upload is refused
  before buffering. Note `field.bytes()` buffers fully in memory; 256 MiB is a
  large allocation and streaming to a temp file under the staging root, hashing
  on the way through, is worth doing in the first cut.

### 9. Admin UI — `crates/ersatzrs-web/src/admin/pages/addons.rs`

- "Install from file" controls for both an add-on package and a repository
  package, rendered only when unverified sources are enabled.
- Trust-tier badge on every repository row and every installed row.
- The available-add-ons section is unaffected — `list_available_addons:605`
  reads only from catalogs, so a sideloaded add-on correctly never appears there
  and has no update path. State that in the UI or it reads as a bug.
- The keyless-repository UX wart found on 2026-08-12 is resolved by this work:
  a keyless official repository must render as "no signing key pinned" with
  Refresh disabled, instead of offering the button and surfacing a raw trust
  error. The maintenance task already skips such repositories
  (`host/tasks.rs:126`).
- The page already carries `#![allow(dead_code, clippy::too_many_lines)]`; do
  not widen it further.

### 10. Localization

New keys in `admin/i18n.rs` plus translations in **all nine** cultures under
`admin/languages/` (`de_de`, `en_us`, `es_419`, `es_es`, `fr_fr`, `it_it`, `nl`,
`pl`, `pt_br`). Budget roughly 12–16 keys — three tier badges, two upload
controls, the setting label/help/confirmation, the no-auto-update note, the
no-key-pinned repository state, and error strings. Per ADR 0012 this is not
optional.

### 11. Health check

`crates/ersatzrs-infra/src/health/checks/addon_platform.rs` — add `TrustTier` to
the installation query (`:33`) and warn on enabled unverified or sideloaded
add-ons, and on unverified repositories. ADR 0066 now makes this an explicit
obligation.

### 12. Tests

- Infra: sideload happy path; oversized package; malformed `addon.toml`;
  zip-slip (already covered by `extract_zip_safely` — assert it holds on the new
  paths too); RID with no entrypoint; sideload blocked when the setting is off;
  sideload refused over a catalog-backed install; sideload-over-sideload
  downgrade refused; `DataNamespace` isolation from a same-ID catalog install.
- Unverified tier: refresh with no key succeeds and records the tier; the same
  repository with a key still verifies; **official + empty key still refuses**;
  tier flip invalidates cached bytes.
- Repository packages: valid manifest installs; unknown field rejected;
  oversized rejected; refused while deployment-managed; refused over an official
  repository.
- Web: multipart route accept/reject, size cap, localized error body.
- Hydration/browser: upload controls appear only when enabled; add to the
  existing all-islands gate.
- `cargo xtask progress check` after updating the parity record.

### 13. Docs and records

- `docs/reference/addons.md` — the `repository.toml` format.
- `docs/reference/addon-repositories.md` — tiers; drop the "seeds an official
  repository row with an empty public key … cannot refresh" paragraph once §3's
  official-repository guard makes that a first-class state.
- `docs/how-to/manage-addons.md` — enabling unverified sources, installing from
  a file.
- `docs/explanation/addon-trust-boundary.md` — the three tiers and what each
  does not promise.
- `docs/progress/parity/addon-platform.toml` — new debt entries, resolved on
  landing.
- `CHANGELOG.md` under `[Unreleased]` → `Added` (on implementation, not for the
  ADR rework alone).

## Effort

| Slice | Estimate |
| --- | --- |
| §1 validation split (own commit) | 0.5 day |
| §2 trust-tier migration | 0.5 day |
| §3 unverified repository tier | 1 day |
| §4–5 sideload install + persistence | 1–1.5 days |
| §6 repository packages | 1–1.5 days |
| §7 setting + boundary | 0.5 day |
| §8–9 upload routes + UI | 1.5–2 days |
| §10 nine-culture localization | 0.5–1 day |
| §11–12 health + tests | 1.5–2 days |
| §13 docs | 0.5 day |
| **Total** | **8.5–11 days** |

Sequencing: §1 → §2 → §3 unlocks the unverified tier alone and is independently
shippable, but note precisely what that does and does not fix. It removes the
key-custody barrier for third-party authors (problem 3). It does **not** help
the private-source or local-development cases (problems 1 and 2): an unverified
catalog must still be fetched anonymously over HTTPS from a publicly trusted
certificate authority, so the artifacts stay public and a local origin stays
unreachable. Those two are fixed only by sideloading.

If the private-source case is the priority, sequence §1 → §2 → §4 → §5 and
defer §3 and §6.

## Risks and open questions

1. **Trust dilution.** The strongest property of the original design was that
   every installed byte traced to a signed catalog. Tiering ends that as a
   universal guarantee. Mitigated by off-by-default, permanent tier recording,
   and health reporting — not eliminated.
2. **The official-repository guard (§3) is easy to miss** and would silently
   downgrade the default source. It deserves a dedicated regression test.
3. **256 MiB in memory.** `field.bytes()` buffers the whole upload; prefer
   streaming to a temp file in the first cut.
4. **Trust on first use for repository packages** is only as good as how the zip
   reached the operator. The UI must not present a key-bearing repository
   package as equivalent to the seeded official trust root.
5. **Support burden.** Every "add-on is broken" report now has two unverified
   branches. The health warning in §11 is what makes that diagnosable.
6. **Localization volume** grows with each tier string; 12–16 keys × 9 cultures
   is the single largest mechanical cost here.

## Explicitly out of scope

- Relaxing `validate_https_url` for local HTTP repositories.
- Custom CA trust for the add-on client (would mean dropping `webpki-roots`).
- Automatic updates, including for verified repositories.
- Highest-compatible-version resolution across repositories.
- A CLI entry point for either install path — admin UI only in the first cut.
- Signing sideloaded packages with a per-installation key.
