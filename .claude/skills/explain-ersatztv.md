---
name: explain-ersatztv
description: Explain an ErsatzTV C# source file or directory without modifying code. Use for read-only summaries of public API, dependencies, behavior, and architectural role before porting.
---

# Explain ErsatzTV

This is a read-only workflow.

## Workflow

1. Read the requested source file or directory under `/home/bart/ErsatzTV/`.
2. Identify public types, methods, dependencies, and collaborators.
3. Explain where the code sits in ErsatzTV's architecture.
4. Call out behavior that matters for ErsatzRS parity.
5. Link to relevant ErsatzRS contracts when useful.

## Output

Keep the explanation practical:

- What it does.
- What it depends on.
- What depends on it.
- Key invariants or edge cases.
- Porting notes for ErsatzRS.
