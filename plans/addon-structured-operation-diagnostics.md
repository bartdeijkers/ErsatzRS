# Plan: give `list` and `play` a structured diagnostic channel

## Implementation status

Implemented on local `main` on 2026-08-15. The shared contract, host-generated
setting and secret codes, list and play consumers, localized admin rendering,
official POSIX and Windows packages, secret-handling regressions, and
architecture guard are present. Verification evidence is recorded under
milestone `ab-02`; this plan remains the implementation-history owner.

## Context

`addon.check.v1` returns `{status, code, message}` with a stable lowercase
`code` (`AddonCheckResult`, `crates/ersatzrs-addon-contract/src/lib.rs:779`).
The `list` and `play` operations have no equivalent: a failure is a non-zero
exit plus free-text stderr, bounded and redacted by `redact_and_bound`
(`crates/ersatzrs-infra/src/addons.rs:3602`).

The host therefore cannot tell one failure from another, and the admin UI cannot
localize any of them. The workaround is in
`crates/ersatzrs-web/src/admin/i18n.rs:2957`:

```rust
if error.contains("required setting CLIENT_ID is not configured")
    || error.contains("environment reference for CLIENT_ID is unavailable")
    || error.contains("Trakt client ID reference is not configured")
```

The first two are host wording from `addons.rs:2481` and `addons.rs:2495`. The
third is an English sentence that lives in a **different repository**:
`native/trakt/src/main.rs:329` in `bartdeijkers/ErsatzRS-addons`. Nothing binds
the two. Rewording that sentence silently disables the Dutch, German, French,
Italian, Spanish, Polish, and Portuguese translation of the most common add-on
failure, with no failing test on either side.

`CLIENT_ID` is likewise not a host concept — it is a setting key declared by one
package's manifest. A third-party add-on with a different key, or any other
failure mode, gets untranslated raw text.

Outcome: add-ons report failures as stable codes, the host localizes on code,
and no host source contains an add-on's prose.

---

## Step 1 — Define the result shape in the contract

Extend `ersatzrs-addon-contract` with one shared failure record reusing the
existing vocabulary:

```rust
pub struct AddonOperationError {
    pub code: String,     // stable, lowercase, validated like AddonCheckResult.code
    pub message: String,  // bounded, add-on-authored, diagnostic only
}
```

Reuse `sanitize_check_code` semantics (`addons.rs:3585`) so codes are validated
identically wherever they appear.

Transport: the last line of stderr, as a single JSON object, is the lowest-cost
option — stdout already carries NDJSON records for `list` and raw media bytes
for `play`, and stderr is already captured, bounded, and redacted. Whatever is
chosen, it must survive the `play` path where stdout is a live media stream.

Reserve a small set of host-recognized codes that any add-on may emit:
`missing-setting`, `missing-secret`, `provider-unreachable`,
`provider-rejected`, `provider-rate-limited`, `not-found`, `unsupported-url`.
Unknown codes are legal and fall back to the add-on's `message`; the code space
belongs to the add-on, the *translated* subset belongs to the host.

## Step 2 — Surface the code through the host

- Carry the code on `AddonError` and on the error paths of
  `InstalledAddonMediaListProvider::enumerate`
  (`crates/ersatzrs-infra/src/addon_media_list.rs`) and playlist enumeration
  (`crates/ersatzrs-scanner/src/remote_stream_playlist.rs`).
- Emit the same codes for host-detected failures so both sides share one
  vocabulary: `resolve_setting_environment` (`addons.rs:2481`, `:2495`,
  `:2524`) already produces exactly `missing-setting` and `missing-secret`
  conditions in prose.
- Keep the bounded, redacted `message` as the diagnostic detail shown under the
  localized line, never as the localized line itself.

## Step 3 — Replace the string matching

Delete `canonical_addon_media_list_error` (`i18n.rs:2957`) and translate on the
code. Add the localized strings for the reserved codes to every supported
culture under `crates/ersatzrs-web/src/admin/languages/`, per the ADR 0012
translation boundary. An unrecognized code renders a generic localized
"the add-on reported a failure" line plus the untranslated `message`.

The `error running server function: ` prefix stripping in the same function is
transport plumbing and is unrelated to the add-on boundary — keep it, moved to
wherever it belongs for server-function errors generally.

## Step 4 — Emit codes from the official packages

In `bartdeijkers/ErsatzRS-addons`:

- `native/trakt/src/main.rs` — replace the prose at `:329` with
  `missing-secret`, and map its HTTP failures onto `provider-unreachable`,
  `provider-rejected`, and `provider-rate-limited`.
- `addons/org.ersatzrs.addon.beeldengeluid` and
  `addons/org.ersatzrs.addon.yt-dlp` — emit the equivalent codes from both the
  POSIX and Windows implementations.

## Step 5 — Regressions

- Each reserved code renders localized text in every supported culture.
- An unknown code renders the generic line plus the bounded message.
- A host-detected missing setting and an add-on-detected missing secret produce
  the same localized outcome.
- Secret redaction still applies to `message`: assert that an environment-backed
  secret value never reaches the rendered string, and that a file-backed secret
  still suppresses add-on-controlled text (`FILE_SECRET_DIAGNOSTIC`,
  `addons.rs:70`).
- A grep-shaped guard asserting no host source contains add-on prose: after this
  slice, no file under `crates/` should match `Trakt client ID` or `CLIENT_ID`
  outside `ersatzrs-addon-contract`'s doc example.

---

## Verification

```bash
./run.sh check -p ersatzrs-addon-contract -p ersatzrs-infra
./run.sh test -p ersatzrs-infra addons
./run.sh test -p ersatzrs-web i18n
cargo clippy -p ersatzrs-web --all-targets -- -D warnings
```

Closeout gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
./run.sh test-closeout
cargo xtask progress check
```

Document the channel in `docs/reference/addon-runtime.md` next to the
`addon.check.v1` contract, and record the outcome in
`docs/progress/parity/addon-host-boundary.toml`.

---

## Out of scope

- A public add-on HTTP or event API. ADR 0066 explicitly excludes one from v1
  and this plan does not reopen it.
- Capability version bumps. The diagnostic channel is additive: an add-on that
  never emits a code keeps today's behavior.
