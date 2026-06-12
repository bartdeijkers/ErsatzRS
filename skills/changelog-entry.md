---
name: changelog-entry
description: Add or adjust a Keep a Changelog entry under Unreleased. Use after any user-visible, contract, feature, fix, security, or build-process change.
---

# Changelog Entry

## Workflow

1. Open [CHANGELOG.md](../CHANGELOG.md).
2. Choose the correct subsection under `[Unreleased]`:
   - `Added`
   - `Changed`
   - `Deprecated`
   - `Removed`
   - `Fixed`
   - `Security`
3. Add a concise user-facing entry.
4. Keep entries in English.
5. Use Conventional Commits as a guide: `feat` usually maps to `Added`, `fix`
   to `Fixed`, behavior changes to `Changed`, removals to `Removed`,
   deprecations to `Deprecated`, and vulnerability work to `Security`.
6. Call out breaking changes explicitly in the relevant entry.
7. Do not create a release section unless performing a release.

## Style

Write entries as project changes, not commit messages.
