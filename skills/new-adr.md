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
   maintainer.
6. Link related contracts, issues, plans, or source files.

## Output

Summarize the new ADR path and any contract files that should link to it.
