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

## Translation boundary

Translate the admin product surface: page headings, form labels, table headers,
buttons, empty/loading states, help text, stable toasts, and validation or
operation errors emitted by admin workflows. Semantic option labels should also
be translated when the value represents a user-facing choice rather than a
technical identifier.

Keep stable identifiers untranslated. This includes product and provider names
such as ErsatzRS, FFmpeg, Plex, Jellyfin, Emby, and Trakt; protocol, container,
codec, and driver names such as MPEG-TS, MP4, MKV, H264, HEVC, AAC, QSV, NVENC,
and VAAPI; configuration keys, routes, SQL/API field names, CSS and HTML
attribute values, search query syntax tokens, and user-authored media, library,
or provider names.

Dynamic diagnostic payloads remain verbatim by default. Raw log messages,
unknown server errors, and health-check details should be preserved for
troubleshooting unless the message is promoted into a stable reusable admin UI
contract with translations in every supported culture.

Development-only or demo-only pages are outside product i18n unless they become
normal admin routes.

## Consequences

The ErsatzRS UI language selector is intentionally wider than upstream ErsatzTV.
Parity claims for the language mechanism should compare storage, normalization,
page refresh behavior, and translated rendering, not strict equality of the
supported-culture list.

Any future import/export or compatibility work that validates `pages.language`
must allow `nl` in ErsatzRS databases while still falling back safely for
unknown values. New admin UI text must include Dutch translations before the
i18n slice can be considered complete.

Future translation work should prioritize untranslated admin product text before
technical identifiers. A literal all-strings translation target is not the
parity contract.

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
