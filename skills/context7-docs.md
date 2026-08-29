---
name: context7-docs
description: Retrieve current, version-specific external documentation through Context7 when ErsatzRS work depends on a library, framework, SDK, CLI, or service contract. Use for API syntax, configuration, feature flags, migrations, and dependency-specific debugging; do not use for repository-owned behavior, ErsatzTV source parity, general programming concepts, or internal business logic.
---

# Context7 Documentation

## Decide whether retrieval adds value

Use Context7 when an answer, implementation, or review finding depends on an
external product's current or version-specific contract, especially for:

- unfamiliar APIs, configuration, feature flags, or CLI behavior;
- version upgrades, migrations, deprecations, and changed defaults;
- compiler or runtime errors that suggest documentation drift; or
- a user request to verify library documentation with Context7.

Start with repository evidence. Inspect the relevant manifest, lockfile,
feature selection, call site, and local contract so the query targets the
dependency and version ErsatzRS actually uses. Do not call Context7 when local
source or tests already settle repository-owned behavior, for a routine
refactor, or for debugging internal business logic. Read `$ERSATZTV_SRC` first
for parity behavior. Use official web sources instead for release status,
security advisories, broad comparisons, or other facts that are not library
documentation.

## Retrieve the smallest useful context

1. Identify the exact package or product and its resolved version locally.
2. Unless the user supplied an exact `/org/project` or
   `/org/project/version` ID, call `resolve-library-id` with both the library
   name and a task-specific natural-language query.
3. Select the exact project when possible. Prefer an indexed version matching
   the lockfile, then strong source reputation, relevant snippet coverage, and
   benchmark score. If only the base project is indexed, include the resolved
   version in the documentation query.
4. Call `query-docs` with the selected `libraryId` and one focused question.
   Name the relevant API, configuration key, error, version, and intended
   outcome. Do not use a single keyword or combine unrelated concepts.
5. Reuse the selected library ID for the rest of the task. Make a separate
   query only for a genuinely distinct concept or when the first result leaves
   one decision-blocking gap.

Budget at most three Context7 calls for one question; the normal path is one
resolution and one documentation query. Stop as soon as the retrieved material
settles the decision. Do not use the removed `get-library-docs`, pagination,
topic, or token-limit arguments from older Context7 versions.

## Apply and report

Treat Context7 as documentation evidence, not proof that the locked dependency
behaves identically. Reconcile the result with local types, enabled features,
compiler output, and tests. When they conflict, prefer observable local behavior
and report the likely version or feature mismatch.

State the library ID and version used when they materially affect the result,
and distinguish documented behavior from an inference. If Context7 has no
credible match, is unavailable, or lacks the required detail, say so briefly
and use the dependency's official documentation or source. Do not install or
configure Context7 unless the user asks.

Context7 queries leave the local environment. Never include secrets, tokens,
private URLs, personal paths, user data, or unnecessary source blobs.
