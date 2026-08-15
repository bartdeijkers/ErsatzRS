# Plan: return provider availability vocabulary to the add-on

## Implementation status

Implemented on 2026-08-15 as milestone `ab-01`. The canonical current contract
is documented in the add-on media-list and Remote Stream playlist references;
delivery evidence lives in `docs/progress/`.

## Context

ADR 0068 states that the generic contracts contain "no provider names,
language-specific keyword tables, or package assets", and the locked ownership
boundary in
[`addon-nfo-sidecars-and-local-replacements-closeout.md`](addon-nfo-sidecars-and-local-replacements-closeout.md)
assigns "provider availability values" to the add-on. The host violates both.

`crates/ersatzrs-core/src/addon_media_list.rs:65` and
`crates/ersatzrs-scanner/src/remote_stream_playlist.rs:205` contain a
byte-identical deserializer that maps six yt-dlp-specific tokens into
`AddonMediaListItemAvailability`:

```rust
Some("available" | "public" | "unlisted") => Available,
Some("unavailable" | "private" | "premium_only" | "subscriber_only" | "needs_auth") => Unavailable,
```

`public`, `unlisted`, `private`, `premium_only`, `subscriber_only`, and
`needs_auth` are the literal values of yt-dlp's `availability` field. They are
in the host because both yt-dlp code paths in `bartdeijkers/ErsatzRS-addons`
emit the raw provider value:

- `addons/org.ersatzrs.addon.yt-dlp/addon.sh` prints `%(availability)j`
  unfiltered in both the media-list and Remote Stream branches;
- `addons/org.ersatzrs.addon.yt-dlp/libexec/youtube-list.ps1:3-9` has an
  `Availability` function that *looks* like a normalizer but returns `$_` — the
  raw token — for every recognized value and only collapses unrecognized ones to
  `unknown`.

Beeld & Geluid already does the right thing: `libexec/beeldengeluid.sh:299-304`
emits `available` / `unavailable` with a separate `availability_reason`. So the
host table exists to serve exactly one package, on both of its platforms.

Two further consequences:

- the mapping is duplicated across two crates, so the two list contracts can
  silently drift apart; and
- any unrecognized token degrades to `Unknown` instead of failing, so a
  misbehaving add-on produces a silently wrong schedule rather than a visible
  error.

Outcome: `available`, `unavailable`, and `unknown` are the only accepted values,
the host holds no provider vocabulary, and the duplicated deserializer is gone.

---

## Step 1 — Normalize inside the yt-dlp package first

Land this in `bartdeijkers/ErsatzRS-addons` before touching the host, so no
release window exists where installed packages emit values the host rejects.

- `addon.sh`: stop passing `%(availability)j` through. yt-dlp's `--print`
  template cannot branch, so switch that path to `--dump-single-json` (as the
  PowerShell path already does) and normalize while emitting, or post-process
  the printed field. Map `public`/`unlisted` to `available`;
  `private`/`premium_only`/`subscriber_only`/`needs_auth` to `unavailable` with
  `availability_reason: "not_playable"`; everything else, including a missing
  or `null` value, to `unknown` with no reason.
- `youtube-list.ps1`: make `Availability` actually return the canonical token
  instead of `$_`, and keep `AvailabilityReason` as-is.
- POSIX and PowerShell must agree token-for-token. The same closeout plan
  already tracks this parity requirement under its section 7; this step is that
  work, made concrete.
- Add package fixtures covering each provider token, a null value, and an
  unrecognized value.

## Step 2 — Delete the host tables

Remove `deserialize_addon_availability` from both
`crates/ersatzrs-core/src/addon_media_list.rs` and
`crates/ersatzrs-scanner/src/remote_stream_playlist.rs`.

`AddonMediaListItemAvailability` (`crates/ersatzrs-domain/src/collection.rs:45`)
already derives `Deserialize` with `rename_all = "snake_case"`, so
`available` / `unavailable` / `unknown` decode without a custom function. Keep
`#[serde(default)]` on the field so v1 records — which omit it — still mean
`available`, and keep that default documented; it is contract behavior, not a
leniency.

Decide explicitly and record the choice in the reference doc: an unrecognized
`availability` token should now **reject the record**, matching how every other
bounded field in these contracts behaves. If a grace period is wanted instead,
log it at warn level with the add-on ID and still reject on the next major
capability version — do not leave a silent coercion.

## Step 3 — Prove the boundary is empty

Add a repository guard so this cannot regress: a test (or an `xtask` check
alongside `architecture-lint`) asserting that no file under `crates/` contains
the provider tokens `premium_only`, `subscriber_only`, `needs_auth`, or
`unlisted`. Provider vocabulary re-entering the host is the exact failure this
plan removes; a grep-shaped guard is proportionate and cheap.

## Step 4 — Regressions

- `available` / `unavailable` / `unknown` decode in both contracts.
- An omitted field decodes as `available` for a v1 record.
- An unrecognized token is rejected with a bounded, provider-neutral message.
- The rejection surfaces through `AddonMediaListProvider::enumerate` and through
  playlist enumeration as an operational failure, not as a silently empty list.

---

## Verification

```bash
./run.sh check -p ersatzrs-core -p ersatzrs-scanner
./run.sh test -p ersatzrs-core addon_media_list
./run.sh test -p ersatzrs-scanner remote_stream_playlist
./run.sh test -p ersatzrs-infra addon_media_list
```

In `bartdeijkers/ErsatzRS-addons`:

```sh
python3 -m unittest discover -s tests
```

Closeout gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
./run.sh test-closeout
cargo xtask progress check
```

Update `docs/reference/addon-media-lists.md` and
`docs/reference/remote-stream-playlist-manifests.md` to state the three accepted
tokens and the rejection behavior, and record the outcome in
`docs/progress/parity/addon-host-boundary.toml`.

---

## Out of scope

- yt-dlp's `ContentKind` keyword heuristics
  (`youtube-list.ps1:11-18`). Those are provider rules that correctly live in
  the package; the POSIX/PowerShell parity gap for them is tracked in the NFO
  closeout plan, not here.
- Any change to what `unknown` means for scheduling. ADR 0068 already decided
  that `unknown` never activates a local replacement.
