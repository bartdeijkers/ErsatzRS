# Run snapshot tests

Use this workflow after changing behavior covered by an `insta` snapshot.

## Run the focused test

Choose the narrowest crate and test filter that regenerate the affected
snapshots:

```bash
./run.sh test -p <crate> <snapshot-test-filter>
```

The test leaves proposed changes as `.snap.new` files.

## Review the changes

Open the interactive review:

```bash
cargo insta review
```

Inspect the scenario name, old value, and new value before accepting or
rejecting each change. Then rerun the focused test.

## Confirm the result

Check that no pending snapshot files remain:

```bash
git status --short
```

Do not commit `.snap.new` files. If the change is not intended, reject the
snapshot and fix the implementation instead of accepting the drift.
