# Parity compatibility pointer

The former hand-written parity matrix no longer owns project state. Structured
feature records, route coverage, ADR relationships, and session history live
under [`docs/progress/`](../progress/README.md). `STATUS.md` is a short pointer
to the same source.

Use the query that matches the lookup:

```bash
cargo xtask progress next
cargo xtask progress parity
cargo xtask progress parity --feature <slug> --full
cargo xtask progress gaps
cargo xtask progress routes
cargo xtask progress adr list
```

Persistence table and real-world decode coverage is tracked separately in the
[persistence coverage tracker](../progress/persistence-gap-matrix.md). This
compatibility page contains no parity data of its own.
