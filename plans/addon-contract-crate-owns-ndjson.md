# Plan: make `ersatzrs-addon-contract` own the NDJSON payloads

> Amended by ADR 0069: the official native Trakt add-on was retired. The
> independently versioned crate, runnable reference add-on, and callable
> conformance checks remain the acceptance proof; no public add-ons workflow
> consumes the private repository.

## Context

ADR 0066 says: "The shared `ersatzrs-addon-contract` crate and add-on contract
reference own these public formats." For the manifest, catalog, repository
manifest, and check result that is true. For the payloads that actually cross
the process boundary on every invocation it is not.

The record types live in two consumer crates instead:

- `AddonMediaListRecord` — `crates/ersatzrs-core/src/addon_media_list.rs:13`
- `PlaylistEntry` (the `remote-stream.list` record) —
  `crates/ersatzrs-scanner/src/remote_stream_playlist.rs:139`

Consequences visible today:

- The two contracts have drifted. `AddonMediaListRecord` carries
  `deny_unknown_fields`; `PlaylistEntry` does not. `PlaylistEntry` tolerates
  `null` arrays through `deserialize_string_vec`; `AddonMediaListRecord` does
  not. The same add-on implements both.
- At review time, the first-party native Trakt add-on hand-built JSON instead
  of consuming the contract crate. ADR 0069 subsequently retired that package
  and its private cross-repository build dependency.
- `ersatzrs-addon-contract` declares `serde_json` in `Cargo.toml` and uses it
  nowhere. The dependency was presumably added for exactly this purpose and then
  the types landed elsewhere.

Outcome: one crate defines every byte that crosses the boundary, with a
runnable reference add-on and callable conformance checks proving its public
Rust surface without registry publication.

---

## Step 1 — Move the record types

Move `AddonMediaListRecord`, `PlaylistEntry`, and the shared enums they need
into `ersatzrs-addon-contract`, re-exporting from the current locations so the
move is mechanical and reviewable.

`AddonMediaListItemAvailability` and `AddonMediaListContentKind` currently live
in `crates/ersatzrs-domain/src/collection.rs:45` and `:61`. They are wire
vocabulary, not domain concepts — the doc comment on `AddonMediaListContentKind`
even says it "does not change the storage identity". Move them to the contract
crate and have `ersatzrs-domain` re-export or newtype them. Check the dependency
direction in `ARCHITECTURE.md` before committing to this: if domain may not
depend on the contract crate, define them in the contract crate and convert at
the infra edge instead.

Keep `EnumeratedAddonMediaList` and `EnumeratedAddonMediaListItem` in
`ersatzrs-core`. Those are validated host-side results, not wire records, and
the split between "what the add-on may send" and "what the host has accepted" is
worth preserving explicitly.

## Step 2 — Make the crate usable outside the workspace

Add a serializer-friendly surface: constructors or builders that make an invalid
record hard to express, plus `validate()` methods mirroring the host's bounds
(non-empty `provider_id`, unique rank, `source_url` required for
`remote_stream`).

External add-ons obtain the crate only from `bartdeijkers/ErsatzRS`. Give each
contract release an immutable `addon-contract-v<version>` tag and have official
Rust add-ons pin its full commit SHA with Cargo's `git` and `rev` dependency
fields. Keep the independent package version as the public API version, set
`publish = false`, and guard that setting so the crate cannot accidentally be
uploaded to crates.io or another registry. Record the exact declaration in
`docs/reference/addon-packages.md` so an author does not have to guess.

Validate a fresh external checkout against the GitHub revision and its committed
lockfile. A local path or Cargo patch is an inner-loop aid only and is not
completion evidence.

Remove the unused `serde_json` dependency if the moved types do not need it.

## Step 3 — Provide a reference consumer

Ship a runnable Cargo example that consumes the same public constructors,
validation, and serializers available to an external Rust add-on. Keep the
crate independently versioned and packageable, but set and guard
`publish = false`. ADR 0069 removes the earlier external native Trakt consumer
and its Git credential requirement from the acceptance contract.

## Step 4 — Ship an executable definition of "valid"

There is currently no fixture, example package, or conformance harness an author
can run. Add, in the contract crate:

- a minimal reference add-on (`examples/`) implementing `addon.check.v1` and
  `media-list.list.v2` against static data; and
- a `#[test]`-callable conformance function that takes captured add-on output
  and asserts it against the contract, so both repositories can exercise the
  same checks.

---

## Verification

```bash
./run.sh check -p ersatzrs-addon-contract
./run.sh test -p ersatzrs-addon-contract
./run.sh test -p ersatzrs-core -p ersatzrs-scanner -p ersatzrs-infra
cargo deny check
```

In `bartdeijkers/ErsatzRS-addons`:

```sh
python3 tools/build_repository.py --sequence 1 --base-url https://example.invalid --output dist
```

Closeout gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
./run.sh test-closeout
cargo xtask progress check
```

---

## Out of scope

- Changing any wire format. This is a relocation plus a published surface; the
  bytes on the boundary must be identical before and after, and the snapshot
  and repository tests are the proof.
- The strictness mismatch between the two record types. It becomes a one-line
  change once both live in one crate, and is tracked separately in
  [`addon-list-record-strictness-parity.md`](addon-list-record-strictness-parity.md).
