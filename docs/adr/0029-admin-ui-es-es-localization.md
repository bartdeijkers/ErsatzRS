# ADR 0029: Add Spanish (Spain) admin UI localization

## Status

Accepted 2026-08-11

## Implementation

Implemented 2026-08-12. The `es-es` translation module, regional metadata,
dynamic helpers, stable error mappings, and tests were added on 2026-08-11; the
2026-08-12 completion pass unified the Playout glossary (which had collided
with Playback), translated the remaining English passthrough returns, and
localized the episode subtitle and streaming-mode labels. The `es-es` leg of
`tests/e2e/settings-i18n.spec.ts` passes in Chromium against a live host.

## Context

ADR 0012 defines the ErsatzRS admin UI localization boundary, persisted
`pages.language` behavior, and the rule that new admin UI text must be translated
through the per-culture language modules. The current language system is
file-driven through `crates/ersatzrs-web/src/admin/languages/`, with language
metadata exposed from each module.

Spanish for Spain is a maintainer-requested admin UI language. It is broader
than the ErsatzTV language list, so it needs an ADR-backed decision before it is
treated as a supported ErsatzRS culture.

## Decision

Add Spanish (Spain) as a first-class ErsatzRS admin UI language.

- Persist the normalized culture code as `es-es` in `pages.language`.
- Render the language option label as `español (España)`.
- Normalize `es`, `es-ES`, and `es_ES` to `es-es`.
- Implement the language in a dedicated admin language module, following the
  metadata and completeness contract used by the existing language files.
- Apply ADR 0012's translation boundary: translate admin product text and stable
  workflow messages, but keep technical identifiers, provider names, codec names,
  routes, configuration keys, logs, and user-authored media data verbatim.

Do not mark this ADR implemented until the Spanish (Spain) language file covers
every admin `TextKey` and the settings/i18n verification path proves the culture
can be selected and rendered.

## Consequences

Spanish-speaking operators in Spain can use the admin UI in their local language
once the implementation lands. The supported culture list intentionally diverges
further from ErsatzTV, so parity comparisons must focus on the persisted-language
mechanism and rendering behavior rather than exact culture-list equality.

Every future admin UI string added after implementation must include a Spanish
(Spain) translation before the i18n coverage gate can be considered complete.

## Alternatives Considered

- Use generic `es` only. Rejected because the request distinguishes Spanish for
  Spain from Latin American Spanish.
- Reuse `es-419`. Rejected because Spain and Latin America need independently
  reviewable translations and display labels.
- Rely on browser translation. Rejected because ErsatzRS uses server-rendered and
  persisted admin UI localization.

## References

- [ADR 0012: Admin UI nl Localization](0012-admin-ui-nl-localization.md)
- `crates/ersatzrs-web/src/admin/languages/`
- `crates/ersatzrs-web/build.rs`
