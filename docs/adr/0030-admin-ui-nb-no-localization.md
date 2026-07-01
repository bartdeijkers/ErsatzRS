# ADR 0030: Add Norwegian Bokmål (Norway) admin UI localization

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

Norwegian for Norway is a maintainer-requested admin UI language. Norwegian has
two official written standards; Bokmål is the practical default locale for a
single Norway-targeted UI language unless Nynorsk is requested separately. This
is broader than the ErsatzTV language list, so it needs an ADR-backed decision
before it is treated as a supported ErsatzRS culture.

## Decision

Add Norwegian Bokmål (Norway) as a first-class ErsatzRS admin UI language.

- Persist the normalized culture code as `nb-no` in `pages.language`.
- Render the language option label as `norsk bokmål (Norge)`.
- Normalize `no`, `no-NO`, `nb`, `nb-NO`, and `nb_NO` to `nb-no`.
- Implement the language in a dedicated admin language module, following the
  metadata and completeness contract used by the existing language files.
- Apply ADR 0012's translation boundary: translate admin product text and stable
  workflow messages, but keep technical identifiers, provider names, codec names,
  routes, configuration keys, logs, and user-authored media data verbatim.

Do not mark this ADR implemented until the Norwegian Bokmål language file covers
every admin `TextKey` and the settings/i18n verification path proves the culture
can be selected and rendered.

## Consequences

Norwegian Bokmål-speaking operators in Norway can use the admin UI in their
local language once the implementation lands. The supported culture list
intentionally diverges further from ErsatzTV, so parity comparisons must focus on
the persisted-language mechanism and rendering behavior rather than exact
culture-list equality.

Every future admin UI string added after implementation must include a Norwegian
Bokmål translation before the i18n coverage gate can be considered complete.

## Alternatives Considered

- Use generic `no-no`. Rejected because `nb-no` is the clearer BCP 47-style tag
  for Norwegian Bokmål in Norway.
- Add Norwegian Nynorsk at the same time. Rejected because the request names
  Norway as one target country and does not ask for two Norwegian written
  standards.
- Rely on browser translation. Rejected because ErsatzRS uses server-rendered and
  persisted admin UI localization.

## References

- [ADR 0012: Admin UI nl Localization](0012-admin-ui-nl-localization.md)
- `crates/ersatzrs-web/src/admin/languages/`
- `crates/ersatzrs-web/build.rs`
