# ADR 0021: Add French (France) admin UI localization

## Status

Accepted 2026-07-17

## Implementation

Implemented 2026-07-17 — French (France) translations live in
`crates/ersatzrs-web/src/admin/languages/fr_fr.rs` (code `fr-fr`, label
`français (France)`, aliases `fr`/`fr-FR`/`fr_FR`), covering every admin
`TextKey`, the dynamic `LanguageStrings` helpers, and the localized
server-error mappings, with ADR 0012's translation boundary preserved.
Coverage is enforced by the `all_languages_translate_each_key` and
supported-scope tests, and the `tests/e2e/settings-i18n.spec.ts` browser proof
verifies French selection, `html lang`, translated route markers, and the
absence of English fallback text across the settings route family. Progress is
tracked in `docs/progress/parity/admin-ui-fr-fr-localization.toml`.

## Context

ADR 0012 defines the ErsatzRS admin UI localization boundary, persisted
`pages.language` behavior, and the rule that new admin UI text must be translated
through the per-culture language modules. The current language system is
file-driven through `crates/ersatzrs-web/src/admin/languages/`, with language
metadata exposed from each module.

French for France is a maintainer-requested admin UI language. It is broader
than the ErsatzTV language list, so it needs an ADR-backed decision before it is
treated as a supported ErsatzRS culture.

## Decision

Add French (France) as a first-class ErsatzRS admin UI language.

- Persist the normalized culture code as `fr-fr` in `pages.language`.
- Render the language option label as `français (France)`.
- Normalize `fr`, `fr-FR`, and `fr_FR` to `fr-fr`.
- Implement the language in a dedicated admin language module, following the
  metadata and completeness contract used by the existing language files.
- Apply ADR 0012's translation boundary: translate admin product text and stable
  workflow messages, but keep technical identifiers, provider names, codec names,
  routes, configuration keys, logs, and user-authored media data verbatim.

Do not mark this ADR implemented until the French language file covers every
admin `TextKey` and the settings/i18n verification path proves the culture can
be selected and rendered.

## Consequences

French-speaking operators in France can use the admin UI in their local language
once the implementation lands. The supported culture list intentionally diverges
further from ErsatzTV, so parity comparisons must focus on the persisted-language
mechanism and rendering behavior rather than exact culture-list equality.

Every future admin UI string added after implementation must include a French
translation before the i18n coverage gate can be considered complete.

## Alternatives Considered

- Use generic `fr` only. Rejected because the requested locale is France-specific
  and future French variants should not be blocked by an ambiguous persisted
  value.
- Rely on browser translation. Rejected because ErsatzRS uses server-rendered and
  persisted admin UI localization.

## References

- [ADR 0012: Admin UI nl Localization](0012-admin-ui-nl-localization.md)
- `crates/ersatzrs-web/src/admin/languages/`
- `crates/ersatzrs-web/build.rs`
