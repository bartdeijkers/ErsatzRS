---
name: new-adr
description: Create the next numbered Architecture Decision Record for ErsatzRS. Use when a change affects architecture, crate boundaries, dependencies across crates, or deliberate ErsatzTV deviations.
---

# New ADR

## Workflow

1. Read [docs/adr/0001-record-architecture-decisions.md](../docs/adr/0001-record-architecture-decisions.md).
2. Find the highest existing `docs/adr/NNNN-*.md`.
3. Copy [docs/adr/0000-template.md](../docs/adr/0000-template.md) to the next
   zero-padded number.
4. Use a lowercase hyphenated filename after the number.
5. Start with `Status: Proposed` unless the decision is already accepted by the
   maintainer. `Status` is the **decision** lifecycle only.
6. Fill in `## Implementation` separately — it is the **code** lifecycle and is
   independent of Status. Use one recognized token (`Not implemented` /
   `Partial — …` / `Implemented (<ref>, <date>)` / `N/A — …`). A freshly
   accepted decision is almost always `Not implemented`; never let
   `Accepted {date}` stand in for "the code exists".
7. If the ADR will land with open follow-up work, record that work as a
   `[[debt]]` in the relevant `docs/progress/parity/<slug>.toml` and add
   `adr = "NNNN"` to that debt. `cargo xtask progress check` then fails if the
   ADR ever claims `Implemented` while the debt is still open.
8. Link related contracts, issues, plans, or source files.

## Output

Summarize the new ADR path and any contract files that should link to it.
