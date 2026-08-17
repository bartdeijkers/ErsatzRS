# Plan: start write transactions immediately

## Implementation status

Not started as a sweep. The mechanism is proven and the pattern is already
applied to the two transactions in `reconcile_tv_projection`
(`crates/ersatzrs-infra/src/repositories/addon_media_list.rs`), which fixed a
reproducible failure on 2026-08-17.

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

- The reproduction test fails against `begin()` and passes against
  `begin_write`.
- No read-only transaction is converted (reviewed per site, not by count).
- `cargo xtask architecture-lint` rejects a newly added writing `pool.begin()`.
- Focused tests per crate, then both clippy passes and `./run.sh test-closeout`.
- Exercise the admin UI under concurrent activity — a library scan running
  while editing lists and playouts — and confirm no `database is locked`
  surfaces.

## Related

- `crates/ersatzrs-infra/src/db.rs::begin_write`
- The SQLite documentation on `BEGIN IMMEDIATE` and why `busy_timeout` cannot
  resolve a deferred-transaction upgrade.
