# Plan: start write transactions immediately

## Implementation status

**Complete (2026-08-18).** Every production transaction that writes now starts
with `crate::db::begin_write`, the one genuinely read-only transaction keeps its
deferred begin with a comment saying why, `cargo xtask architecture-lint`
guards the default, and both the deferred-upgrade failure and the television
projection have regression tests.

What the triage found, against the estimates below:

- **100 production `.begin()` sites**, not 126. The remaining 33 of the 133
  matches are test code: 7 in `crates/*/tests/`, and 26 in inline
  `#[cfg(test)]` / `#[cfg(all(test, …))]` modules. Notably *all 11*
  `ersatzrs-web` sites are test-only `…_with_pool` duplicates of server
  functions whose production path goes through `ersatzrs-app` and the infra
  repositories, so the web crate had nothing to convert.
- **98 sites converted.** 100 minus the `begin_write` helper's own non-SQLite
  fallback in `db.rs`, minus the one read-only transaction.
- **1 read-only transaction**, not 2:
  `ersatzrs-scanner/src/local_television.rs::reconcile_merged_local_shows`,
  which takes a consistent snapshot of the owned episodes via
  `load_owned_episodes` and commits without writing. It keeps `begin()` and
  carries a comment explaining that `begin_write` would serialise readers.
  `scanner/library_folder.rs` turned out to be a test, and
  `media_server/jellyfin_emby.rs` writes.
- **0 pool-inside-transaction sites.** Step 2 below was checked and is a no-op:
  no production transaction body queries the pool (`&self.0`, `&pool`,
  `&self.pool`) rather than the transaction, so no site had its failure mode
  changed from fail-fast to a potential stall. No transaction body awaits
  non-database work (HTTP, filesystem, subprocess) either, so no converted
  transaction holds the write lock across slow work.

## Context

SQLite starts an ordinary transaction in **deferred** mode. It takes a read
snapshot on its first statement and only acquires the write lock when it first
writes. If another connection commits in between, that upgrade fails
**immediately** with `SQLITE_BUSY` — and `busy_timeout` deliberately does not
apply, because waiting can never resolve it: the transaction holds a stale
snapshot and must be abandoned.

Almost every write path here reads before it writes ("does this row exist? …
then insert"), which is exactly the vulnerable shape.

Measured on the affected machine — `busy_timeout` was 30 s in both cases:

```
DEFERRED  BEGIN + SELECT, concurrent commit, then INSERT
    -> FAILED after 0.000s: database is locked
BEGIN IMMEDIATE + SELECT, concurrent commit, then INSERT
    -> succeeded in 0.000s
```

That matches the production symptom exactly: a 500 with
`database is locked` and a latency of **15 ms** — not a timed-out wait. The
pool runs sqlx's default of 10 connections and the admin UI issues several
server functions per page, so a concurrent write is easy to hit. The operator
reported the same error appearing elsewhere in the admin UI.

`crates/ersatzrs-infra/src/db.rs::begin_write` now exists for this: it issues
`BEGIN IMMEDIATE` on SQLite, where `busy_timeout` *does* apply, so writers queue
instead of failing. It leaves the MySQL path alone, where the spelling is
invalid.

## Scope

126 `.begin()` sites outside tests:

| crate | sites |
|---|---:|
| `ersatzrs-infra` | 93 |
| `ersatzrs-scanner` | 22 |
| `ersatzrs-web` | 11 |

Densest files: `ersatzrs-scanner/src/lib.rs` (15),
`ersatzrs-infra/src/repositories/collection.rs` (13),
`plex.rs` / `jellyfin/persistence.rs` / `emby/persistence.rs` (8 each).

A first pass looking for `INSERT`/`UPDATE`/`DELETE` within 25 lines of each
`begin()` suggests only about **2** are plausibly read-only
(`media_server/jellyfin_emby.rs`, `scanner/library_folder.rs`). So this is
mostly mechanical — but it is deliberately *not* a blind find-and-replace,
for two reasons below.

## Why this is triaged rather than swept

1. **Read-only transactions must keep the deferred begin.** `BEGIN IMMEDIATE`
   takes the write lock, so applying it to a read-only transaction serialises
   readers that WAL would otherwise let run concurrently. Each site must be
   confirmed to write.
2. **A transaction that acquires a second pool connection changes failure
   mode.** Today such a path fails fast; holding the write lock first turns it
   into a wait of up to `busy_timeout`, and if the transaction is itself
   waiting on that second connection, into a stall. Any site that queries the
   pool (rather than the transaction) inside its transaction body must be
   fixed, not merely converted. Both projection transactions were checked for
   this before conversion and were clean.

## Approach

1. **Enumerate and classify.** For each site record: does the body write; does
   the body touch the pool rather than the transaction; is it hot (request
   path) or batch (scan). A small `xtask` or ripgrep script can produce the
   inventory; the write/no-write judgement is a read of each body.
2. **Fix any pool-inside-transaction sites first**, independently — those are
   latent stalls regardless of begin mode.
3. **Convert the writers** to `crate::db::begin_write`, highest-traffic first:
   admin repositories (`collection.rs`, `program_schedule.rs`, `playout.rs`,
   `channel.rs`, `block.rs`), then `addons.rs` / `addon_media_storage.rs` /
   `addon_media_list_sync.rs`, then the scanners and media-server persistence.
4. **Leave read-only transactions on `begin()`**, with a brief comment saying
   why, so the next reader does not "fix" them.
5. **Guard the default.** Add an `architecture-lint` rule that flags a new
   `pool.begin()` whose body writes, pointing at `begin_write`. Without this
   the pattern returns with the next feature.

The implemented rule is deliberately stricter than "prove the body writes",
because proving it needs transitive analysis through helpers that take the
transaction, and a lint that guesses wrong in that direction fails silently.
Instead every production `.begin().await` must be either `begin_write` or
annotated with a `read-only transaction` comment; an annotated site whose body
still contains write SQL is reported as a wrong annotation. The only exemption
is `crates/ersatzrs-infra/src/db.rs`, where `begin_write` falls back to a plain
`begin()` on backends that reject `BEGIN IMMEDIATE`.

## Regression coverage

There is currently **no test** exercising `reconcile_tv_projection`, which is
how a fully broken projection reached a running system.

- A focused test that reproduces the deferred-upgrade failure: open a
  transaction, commit from a second connection, then write — asserting the
  `begin_write` path survives and (as a control) that the deferred path is the
  one that fails.
- A projection test covering show/season/episode creation, including the
  `SeasonMetadata` row whose absence made projected seasons invisible.
- Both belong in `ersatzrs-infra`, using the existing temp-SQLite pattern.

## Verification

Done:

- `crates/ersatzrs-infra/tests/write_transaction.rs` reproduces the failure on
  a file-backed database with the production pool settings: a deferred
  transaction that reads, sees a concurrent commit, and then writes fails
  immediately with `database is locked` (asserted to fail fast, not to wait out
  `busy_timeout`), while `begin_write` completes and a concurrent writer queues
  behind it rather than failing.
- `crates/ersatzrs-infra/tests/addon_media_list_tv_projection.rs` covers
  show/season/episode creation including the `SeasonMetadata` row whose absence
  made projected seasons invisible, projection idempotency, and reverting to
  Remote Streams.
- No read-only transaction was converted; each of the 100 sites was classified,
  and the 21 without literal write SQL in the body were read individually.
- `cargo xtask architecture-lint` rejects a newly added writing `pool.begin()`
  — verified both by unit tests and by reintroducing one in
  `repositories/block.rs` and watching the lint fail.
- `cargo fmt`, both clippy passes, `./run.sh test-closeout` (1573 passed), and
  `cargo xtask smoke` all pass.

Remaining (manual, not automatable here):

- Exercise the admin UI under concurrent activity — a library scan running
  while editing lists and playouts — and confirm no `database is locked`
  surfaces.

## Related

- `crates/ersatzrs-infra/src/db.rs::begin_write`
- The SQLite documentation on `BEGIN IMMEDIATE` and why `busy_timeout` cannot
  resolve a deferred-transaction upgrade.
