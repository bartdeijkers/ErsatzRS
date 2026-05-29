# ADR 0012: Admin UI nl Localization

## Status

Accepted

## Context

ErsatzTV persists the admin UI culture in `ConfigElement` under
`pages.language`. Its request culture provider reads that value and uses it for
localized UI resources. Upstream ErsatzTV currently exposes English
(`en-us`), Polish (`pl`), and Portuguese (Brazil) (`pt-br`) in
`Localization.SupportedLanguages`.

ErsatzRS needs the same persisted-language behavior for parity, but the
maintainer also requires Dutch (the Netherlands) as a first-class admin UI
language. This extends the upstream language list while preserving the same
storage key, request/session behavior, and settings route.

## Decision

Support `nl` in ErsatzRS as an intentional admin UI localization extension.
This is standard Dutch for the admin UI.
Persist it as the normalized culture code `nl` in `pages.language`, list it
on `/settings/ui` as `Nederlands`, and render the admin UI from the
same translation lookup path used by the ErsatzTV-supported cultures.

Keep `en-us` as the default and normalization fallback. Continue to support
ErsatzTV's `pl` and `pt-br` values verbatim. Normalize `nl`, `nl-NL`, and
`nl_NL` input to `nl`.

In the future we may also support Flemish Dutch as `nl-be`, but that is out of
scope for now.

## Consequences

The ErsatzRS UI language selector is intentionally wider than upstream ErsatzTV.
Parity claims for the language mechanism should compare storage, normalization,
page refresh behavior, and translated rendering, not strict equality of the
supported-culture list.

Any future import/export or compatibility work that validates `pages.language`
must allow `nl` in ErsatzRS databases while still falling back safely for
unknown values. New admin UI text must include Dutch translations before the
i18n slice can be considered complete.

## Alternatives Considered

- Match ErsatzTV's language list exactly. Rejected because Dutch UI support is
  an explicit ErsatzRS requirement.
- Implement Dutch as a browser-only override. Rejected because ErsatzTV's
  behavior is server-driven through persisted `pages.language`, and SSR output
  must reflect the configured language.

## References

- `$ERSATZTV_SRC/ErsatzTV/Localization.cs`
- `$ERSATZTV_SRC/ErsatzTV/DatabaseRequestCultureProvider.cs`
- `$ERSATZTV_SRC/ErsatzTV/Pages/Settings/UISettings.razor`
- [Settings parity slice](../../plans/settings-parity-slice.md)
