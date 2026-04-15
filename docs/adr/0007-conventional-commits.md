# ADR 0007: Use Conventional Commits

## Status

Accepted

## Context

ErsatzRS needs a changelog and release process that can scale from solo development
to public open-source collaboration.

## Decision

Use Conventional Commits for all commits and enforce them in CI once the public
workflow is active.

## Consequences

History becomes easier to scan and automate. Contributors need to choose a type
and scope for every commit.

## Alternatives Considered

- Free-form commit messages: rejected because they make release notes harder to
  generate and review.

## References

- [../../CONTRIBUTING.md](../../CONTRIBUTING.md)
- [../../commitlint.config.js](../../commitlint.config.js)
