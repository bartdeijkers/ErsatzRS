# ADR 0036: Add Croatian (Croatia) admin UI localization

## Status

Proposed

## Implementation

Not implemented

## Context

ADR 0012 defines the ErsatzRS admin UI localization boundary, persisted
`pages.language` behavior, and the rule that new admin UI text must be translated
through the per-culture language modules. The current language system is
file-driven through `crates/ersatzrs-web/src/admin/languages/`, with language
metadata exposed from each module.

Croatian is part of the requested broader European admin UI language coverage.
It is broader than the ErsatzTV language list, so it needs an ADR-backed
decision before it is treated as a supported ErsatzRS culture.

## Decision

Add Croatian (Croatia) as a first-class ErsatzRS admin UI language.

- Persist the normalized culture code as `hr-hr` in `pages.language`.
- Render the language option label as `hrvatski (Hrvatska)`.
- Normalize `hr`, `hr-HR`, and `hr_HR` to `hr-hr`.
- Implement the language in a dedicated admin language module, following the
  metadata and completeness contract used by the existing language files.
- Apply ADR 0012's translation boundary: translate admin product text and stable
  workflow messages, but keep technical identifiers, provider names, codec names,
  routes, configuration keys, logs, and user-authored media data verbatim.

Do not mark this ADR implemented until the Croatian language file covers every
admin `TextKey` and the settings/i18n verification path proves the culture can
be selected and rendered.

## Consequences

Croatian-speaking operators can use the admin UI in their local language once
the implementation lands. The supported culture list intentionally diverges
further from ErsatzTV, so parity comparisons must focus on the persisted-language
mechanism and rendering behavior rather than exact culture-list equality.

Every future admin UI string added after implementation must include a Croatian
translation before the i18n coverage gate can be considered complete.

## Alternatives Considered

- Use generic `hr` only. Rejected because the requested coverage is
  country-specific and future Croatian variants should not be blocked by an
  ambiguous persisted value.
- Rely on browser translation. Rejected because ErsatzRS uses server-rendered and
  persisted admin UI localization.

## References

- [ADR 0012: Admin UI nl Localization](0012-admin-ui-nl-localization.md)
- `crates/ersatzrs-web/src/admin/languages/`
- `crates/ersatzrs-web/build.rs`
