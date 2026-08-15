# Plan: validate the operator source URL and make `network_hosts` mean something

## Context

The add-on platform is strict about every URL it controls.
`ersatzrs-addon-contract` rejects non-HTTPS URLs, userinfo, and — for package
URLs — query strings and fragments (`is_https_url`, `is_package_url`,
`crates/ersatzrs-addon-contract/src/lib.rs:934-947`). Catalog, package, icon,
source, and panel-link URLs all pass through it.

The one URL an operator types, and the only one the host hands to a child
process, is not checked at all:

- `add_addon_media_list` (`crates/ersatzrs-app/src/commands.rs:4390`) rejects
  only an empty string, then passes the value to the repository, which places it
  in `ERSATZRS_MEDIA_LIST_URL`.
- `parse_playlist_manifest`
  (`crates/ersatzrs-scanner/src/remote_stream_playlist.rs:332`) rejects only an
  empty `url`, then places it in `ERSATZRS_REMOTE_STREAM_PLAYLIST_URL`.

Today the add-ons defend themselves — `addon.sh` in the yt-dlp package rejects
non-HTTP(S) schemes and quoting characters — which means the guarantee depends
on each package remembering to implement it.

Separately, `AddonPermissions` has four fields and only one has runtime meaning.
`managed_ffmpeg` genuinely gates `FFMPEG_BIN`/`FFPROBE_BIN`
(`crates/ersatzrs-infra/src/addons.rs:2530`). `network_hosts`, `media_read`, and
`external_commands` appear only in the admin renderer
(`crates/ersatzrs-web/src/admin/pages/addons.rs:1094-1097`). ADR 0066 is honest
that permissions are "not a portable operating-system sandbox", and that stays
true — but the source URL is a host-controlled input flowing to an add-on that
already declares which hosts it expects. Checking one against the other is
cheap, is not a sandbox claim, and catches the realistic mistake: a link pasted
into the wrong provider's list.

Outcome: an operator-supplied URL is validated once, in the host, and a package
that declares specific hosts cannot be pointed at a different one.

---

## Step 1 — Validate the URL in the host

Add one shared validator to `ersatzrs-addon-contract` beside the existing URL
helpers and use it on both entry points.

Decide the accepted scheme set explicitly. HTTPS-only matches the rest of the
platform; HTTP is likely still needed for a local or on-premise provider. Pick
one, state it in the reference docs, and reject everything else — in particular
`file:`, which is the reason this matters. Reject userinfo, as the platform does
everywhere else, so credentials cannot be persisted in a list row or in a
scanned YAML manifest.

Validate in `ersatzrs-app` alongside the existing empty check, so the rejection
is a normal localized validation error before any provider call, and again in
`parse_playlist_manifest` so a hand-authored manifest cannot bypass it.

## Step 2 — Enforce `network_hosts` for that URL

When the installed manifest declares `network_hosts` and the list does not
contain `*`, require the URL's host to match one entry. Support the same
leading-`.` or `*.`-style suffix form the manifests already use, and define it
in `docs/reference/addon-packages.md` — the contract currently validates the
*shape* of a host pattern (`lib.rs:387`) without ever saying what it matches.

Frame it precisely in the docs: this is a host-side input check on one operator
value, not confinement. The add-on can still contact anything; ADR 0066's
"What trust does not mean" section stays correct and should be cross-referenced
rather than weakened.

## Step 3 — Report it as a normal add-on failure

A rejected URL is an operator error, not a provider outage. Give it a reserved
code from
[`addon-structured-operation-diagnostics.md`](addon-structured-operation-diagnostics.md)
(`unsupported-url`) so it renders localized in the Add-on Lists surface and in
the Remote Stream playlist health check, and make sure the message names the
offending host without echoing the whole URL.

## Step 4 — Regressions

- `file:`, `javascript:`, and a scheme-less string are rejected at import and at
  manifest parse.
- A URL with userinfo is rejected.
- A URL whose host is absent from a restrictive `network_hosts` list is
  rejected; the same URL is accepted when the manifest declares `*`.
- Suffix patterns match the intended subdomains and nothing else.
- An existing saved list whose URL would now fail keeps rendering and reports
  the failure, rather than disappearing from administration.

---

## Verification

```bash
./run.sh check -p ersatzrs-addon-contract -p ersatzrs-app -p ersatzrs-scanner
./run.sh test -p ersatzrs-app addon_media_list
./run.sh test -p ersatzrs-scanner remote_stream_playlist
./run.sh test -p ersatzrs-web addon_lists
```

Closeout gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
./run.sh test-closeout
cargo xtask progress check
```

Update `docs/reference/addon-packages.md` (host-pattern semantics),
`docs/reference/addon-media-lists.md`, and
`docs/reference/remote-stream-playlist-manifests.md`; record the outcome in
`docs/progress/parity/addon-host-boundary.toml`.

---

## Migration note

Enforcement can reject URLs that already work. Before landing Step 2, check
whether the official packages' declared hosts actually cover every link they
support — `org.ersatzrs.addon.beeldengeluid` declares exactly
`schatkamer.beeldengeluid.nl` and `sk-video.cdn.beeldengeluid.nl`. If a legal
Schatkamer link uses another host, the manifest is what must change, not the
enforcement.

---

## Out of scope

- Making `media_read` and `external_commands` enforceable. Neither is a host
  input; enforcing them needs an OS sandbox, which ADR 0066 rejected for v1.
  They remain declarations, and the UI already presents them as such.
- `ERSATZRS_REMOTE_STREAM_URL` for `play`. That value originates from add-on
  output the host already validated on ingest; revisit only if hand-authored
  Remote Stream YAML is found to reach it unchecked.
