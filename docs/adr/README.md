# Architecture decision records

ADRs record durable technical and product decisions outside the four Diátaxis
documentation types. They own context, the selected option, alternatives,
consequences, lifecycle, and supersession; they do not prove implementation.

Before adding or changing a decision:

1. read [ADR 0001](0001-record-architecture-decisions.md);
2. copy the [ADR template](0000-template.md) and use the next available number;
3. record delivery status and remaining debt in `docs/progress/`; and
4. validate the derived index with `cargo xtask progress check`.

List current decisions and their derived implementation state with:

```bash
cargo xtask progress adr list
```

Do not maintain a second hand-written ADR table here; the query derives it from
the ADR files and parity records.
