# Plan: migrate the official repository publisher to Rust

## Context

Milestone ab-06 removes manifest-contract validation from
`tools/build_repository.py` in `bartdeijkers/ErsatzRS-addons` and makes the
host validator authoritative. What remains in Python is publication policy and
artifact construction: discover package files, stage optional native artifacts,
write deterministic ZIP archives, normalize packaged Windows batch files to
CRLF, embed the repository license, copy and describe icons, calculate package
hashes and sizes, and assemble the unsigned repository catalog.

Those jobs do not need to remain in Python. Moving them to Rust gives the
official repository one typed, testable publisher while preserving the more
important ownership rule established by ab-06: publication code may consume a
validated manifest, but it must not implement a second manifest validator.

The publisher belongs to the public add-ons repository because repository
identity, package selection, artifact layout, and publication policy belong to
that repository rather than to the ErsatzRS runtime. It must not add a build
dependency or CI credential for the private ErsatzRS repository. It consumes
the pinned validator mechanism delivered by ab-06 at the process boundary.

Outcome: the official repository no longer uses Python to construct publication
artifacts, and the Rust migration is proven against the existing deterministic
output before the Python builder is removed.

---

## Step 1 — Freeze the current publication behavior

Before replacing the builder, capture migration fixtures with a fixed sequence,
base URL, generation time, package tree, icon, and representative native
artifact layout. Assert all publication properties that must survive:

- stable file discovery and lexical archive ordering, excluding hidden paths;
- the fixed ZIP timestamp, compression level, and Unix modes;
- repository `LICENSE` injection and package-local duplicate handling;
- CRLF normalization for packaged `.bat` files;
- portable `any` packages and one package per declared RID when native
  entrypoints exist;
- package and icon SHA-256 values and byte sizes;
- deterministic icon names, package URLs, add-on ordering, compact sorted JSON,
  the monotone sequence, and the 21-day validity interval.

Build the same fixture twice and require byte-for-byte equality for every file.
Also retain one old-Python/new-Rust comparison fixture during migration. That
comparison must prove equivalent file inventories, extracted package bytes,
metadata, hashes, sizes, ordering, and validity; it need not require two
different DEFLATE implementations to emit identical compressed bytes. The
comparison may be removed with the Python implementation only after the Rust
tests independently assert every frozen property.

## Step 2 — Add a repository-owned Rust CLI

Create a small Rust binary in the add-ons repository, for example
`tools/repository-publisher/`, add it to a repository-root Cargo workspace, and
check in the resulting lockfile. Preserve the existing command-line inputs:

```text
--sequence <positive integer>
--base-url <https URL>
--output <directory>
--generated-at <RFC 3339 timestamp, optional>
--native-artifacts <directory, optional>
```

Keep repository publication policy in this binary and keep add-on contract
policy out of it. The publisher must invoke or consume the pinned ab-06 host
validator before reading a manifest for publication. Its own checks are limited
to publication inputs and filesystem facts: manifest ID matching the package
directory, icon existence/type/size, required declared artifacts, safe output
paths, sequence and base-URL constraints, and deterministic catalog assembly.

Use the contract-owned repository index shape or validate the generated index
through the pinned host tool before it can become publishable. Do not duplicate
`AddonManifest::validate()` rules in publisher types or helper functions.

Construct the complete output in a staging directory beside the destination and
replace the destination only after every package, icon, hash, and catalog has
been produced and validated. A failed build must not leave a partially updated
publication tree.

## Step 3 — Port publication regressions

Move the repository-builder assertions from `tests/test_repository.py` into
Rust integration tests. Keep provider entrypoint and fixture tests in their
current language where they are unrelated to publication.

Cover at least:

- two identical builds produce identical bytes;
- the generated index and packages agree on URL, hash, size, RID, and version;
- package entries have the frozen timestamp and modes;
- every packaged batch file contains CRLF line endings only;
- icons are bounded PNG files and their copied bytes match catalog metadata;
- missing native artifacts, unsafe paths, invalid publication inputs, and
  malformed validated input fail before destination replacement;
- the generated catalog is accepted by the host contract validator;
- no manifest rejected by the ab-06 validator reaches package construction.

## Step 4 — Switch publication CI and retire Python construction

Build the Rust publisher with a checked-in lockfile and run it in
`.github/workflows/publish.yml` after the pinned ab-06 validation step. Pin all
cross-repository tools or artifacts by immutable version and digest according to
the mechanism selected by ab-06; do not grant the public workflow credentials
for the private host repository.

After comparison and Rust regressions pass, delete
`tools/build_repository.py` and remove Python imports that existed only to test
it. Python may remain for provider-specific tests, but it is no longer part of
constructing repository packages or the catalog.

The Rust publisher emits unsigned exact catalog bytes. Detached signing remains
a separate downstream CI step that receives the existing CI-secret reference;
the publisher never accepts, reads, logs, or persists a private key. GitHub
Pages upload and deployment also remain downstream orchestration.

## Step 5 — Document and close the migration

Update the add-ons repository contributor instructions and publication workflow
documentation with the Rust command, deterministic-input contract, and local
unsigned build procedure. Update the ErsatzRS add-on package/repository
reference only where the supported author workflow changes.

Record completion in
`docs/progress/parity/addon-host-boundary.toml`, mark milestone ab-07 complete
with exact test and CI evidence, and prune this standalone plan once it is no
longer actionable.

---

## Verification

In `bartdeijkers/ErsatzRS-addons`:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 -m unittest discover -s tests
cargo run --locked --manifest-path tools/repository-publisher/Cargo.toml -- \
  --sequence 1 \
  --base-url https://example.invalid \
  --output dist \
  --generated-at 2026-08-16T00:00:00Z
```

Validate every produced ZIP, independently recalculate catalog hashes and
sizes, and run the host-owned validator against all manifests and the generated
catalog. In ErsatzRS, run:

```bash
cargo xtask progress check
```

Publication closeout requires a terminal add-ons workflow for the exact commit
that uses the Rust publisher. Signing success may be reported only from CI
status and non-sensitive artifact metadata; private-key material and
secret-bearing logs stay outside agent context.

---

## Out of scope

- Changing manifest, package, catalog, capability, or trust semantics.
- Moving official-repository policy into the ErsatzRS runtime or host workspace.
- Replacing the ab-06 validator or adding another manifest validator.
- Changing the Ed25519 key, provisioning a production key, or exposing signing
  material to the publisher, tests, logs, or agent tooling.
- Replacing detached signing, GitHub Pages deployment, or provider entrypoint
  implementations.
- Restoring Trakt or another native add-on; native artifact support is retained
  as a generic publication capability only.
