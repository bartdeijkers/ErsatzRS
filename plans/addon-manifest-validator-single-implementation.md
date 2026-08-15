# Plan: one manifest validator instead of two

## Context

`AddonManifest::validate()` (`crates/ersatzrs-addon-contract/src/lib.rs:96`) is
the host's gate: schema-version gating, reverse-DNS identity, required `en-US`
entries, HTTPS `source_url`, capability-ID grammar, dependency rules, RID and
entrypoint path safety, setting-key grammar, secret-reference rules,
`media_list_storage` prerequisites, and permission bounds.

`tools/build_repository.py` in `bartdeijkers/ErsatzRS-addons` implements the
same contract a second time, in Python. `validate_manifest` covers unknown root
fields, manifest versions `{1, 2, 3}`, the icon-requires-v2 rule, `en-US`
requirements, the HTTPS `source_url` prefix, the `addon.check.v1` requirement,
the capability regex `[a-z0-9.-]+[.]v[0-9]+`, dependency rules, entrypoints,
settings, the `suggested_reference`-requires-v3 rule, the
`media_list_storage` capability set, and external-command names.

Two implementations of one contract, in two languages, in two repositories, kept
in sync by hand. There is no shared test. Drift does not fail either CI — it
fails at install time on a user's machine, as a rejected package from the
official repository.

The Python version is already a slightly different contract. It checks
`source_url.startswith("https://")` where the Rust version parses the URL and
also rejects userinfo. It requires `manifest id == directory name`, which the
Rust contract does not know about. Neither difference is wrong, but nothing
records which is authoritative.

Outcome: the add-ons repository validates against the host's real validator, and
the Python builder keeps only the rules that are genuinely
publication-specific.

---

## Step 1 — Expose the validator as a tool

Add a small binary to the host repository — an `xtask` subcommand or a thin bin
in `ersatzrs-addon-contract` — that reads one or more `addon.toml` (and
`repository.toml`) files and exits non-zero with the exact `ContractError` text:

```bash
cargo xtask validate-addon-manifest addons/org.ersatzrs.addon.trakt/addon.toml
```

This is the same code path the host runs at install time, so a pass is a real
guarantee rather than an approximation. It also gives third-party authors a way
to check a package before publishing, which today does not exist.

Depends on
[`addon-contract-crate-owns-ndjson.md`](addon-contract-crate-owns-ndjson.md)
deciding how the contract crate is consumed from outside the workspace; the same
mechanism (published crate or pinned git tag) carries this tool.

## Step 2 — Call it from the add-ons repository CI

Add a step to `.github/workflows/publish.yml` in
`bartdeijkers/ErsatzRS-addons` that validates every `addons/*/addon.toml` with
the host tool before packaging. Pin the host version explicitly, so the
repository states which ErsatzRS contract it builds against — the manifests
already declare `host_version = ">=0.1.0, <0.2.0"` and the CI should agree with
that rather than track `main` implicitly.

## Step 3 — Reduce the Python validator to what only it knows

`build_repository.py` keeps the rules that are about *publication*, not about
the manifest contract:

- `manifest id == directory name`;
- the icon file exists, is within the size bound, and matches its declared type;
- deterministic ZIP construction and package digests;
- catalog assembly, sequence, and validity interval;
- the required native artifacts are staged for all six RIDs.

Delete the duplicated contract checks. Where a Python check is stricter than the
Rust one and that strictness is wanted — the `source_url` prefix is not, but the
directory-name rule may be — decide where it belongs. If it belongs to the
contract, move it into `AddonManifest::validate()` so the host enforces it too.
If it belongs to publication, keep it in Python and say so in a comment.

## Step 4 — Reconcile the known differences

Resolve each explicitly rather than leaving both behaviors alive:

- `source_url`: the Rust parse-based check is stricter and correct; drop the
  Python prefix check.
- `id == directory name`: publication-only, or contract? Record the decision.
- Any rule present in Rust but absent in Python is currently unenforced at build
  time — enumerate them once as part of this slice, because they are the ones
  that will first surface as a user-visible install failure.

## Step 5 — Regressions

- A manifest that the host would reject fails the add-ons repository build.
- A manifest that the host accepts builds and publishes.
- The `tests/test_repository.py` suite still covers packaging determinism, icon
  handling, and catalog assembly after the contract checks are removed.

---

## Verification

```bash
cargo xtask validate-addon-manifest <manifest>   # once the subcommand exists
./run.sh test -p ersatzrs-addon-contract
```

In `bartdeijkers/ErsatzRS-addons`:

```sh
python3 -m unittest discover -s tests
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

Document the tool in `docs/reference/addon-packages.md` as the supported way for
an author to check a package, and record the outcome in
`docs/progress/parity/addon-host-boundary.toml`.

---

## Out of scope

- Replacing the Python tooling wholesale. Deterministic ZIP building, icon
  processing, catalog assembly, and Ed25519 signing work today and are not part
  of the contract-duplication problem.
- Signing-key handling. Provisioning the production key remains the
  `official-addon-repository-is-not-published` debt in
  `docs/progress/parity/addon-platform.toml`, and stays outside agent context.
