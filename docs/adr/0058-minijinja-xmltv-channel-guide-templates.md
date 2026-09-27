# ADR 0058: minijinja for XMLTV channel-guide template customization

## Status

Accepted (2026-08-01)

## Implementation

Implemented (2026-08-01). `/iptv/xmltv.xml` renders `<channel>`/`<programme>`
elements through seven `minijinja` templates (one per `MediaItem` kind plus
one for `<channel>`), with a custom override file in
`<data>/templates/channel-guide/` beating the bundled default of the same
name. `ersatzrs-infra::xmltv::load_guide_items` resolves rich per-kind
content (plot, genres, content rating, artwork, episode numbering, artist
name) for all distinct media items in bounded batches per guide request,
riding alongside the existing block-behavior timing projection in
`ersatzrs-core::xmltv`.

## Context

ErsatzTV lets administrators customize `/iptv/xmltv.xml` output per media
kind by dropping a Scriban (`.sbntxt`) template into
`FileSystemLayout.ChannelGuideTemplatesFolder`, overriding the bundled
default (`movie.sbntxt` overrides `_movie.sbntxt`). Its handler
(`RefreshChannelDataHandler.cs`) builds a flat per-kind data object and hands
it to the Scriban engine; the template controls which XMLTV elements appear.

ErsatzRS's XMLTV guide emitted only a bare `<title>` per `<programme>` and a
bare `<display-name>` per `<channel>` — none of the plot/genre/rating/
artwork/episode-numbering metadata the scanner already persists reached the
guide, making third-party EPG clients (Kodi and similar) show almost no
information. There is no Scriban (or equivalent) crate in the Rust
ecosystem, and this codebase's one existing "Scriban template" consumer
(`music_video_credits.rs`) hand-rolls a narrow string-search interpreter
tailored to its own small, fixed set of bundled ASS templates — not a
general engine suitable for free-form user-authored XMLTV templates with
loops over genres and conditional optional fields.

## Decision

Port ErsatzTV's *pattern* — bundled per-kind defaults, a same-name override
file wins, flat snake-case context variables named after ErsatzTV's own
(`movie_title`, `movie_plot`, `show_title`, ...) — rather than its Scriban
*syntax*, onto `minijinja` (Jinja2-compatible, MIT/Apache-2.0, minimal
dependency footprint). Bundled defaults live in
`assets/templates/channel-guide/_*.jinja` and are compiled into the binary
via `include_str!`, mirroring `music_video_credits_templates_folder`'s
existing bundled-template convention in this codebase rather than ErsatzTV's
own extract-to-disk-at-startup `ResourceExtractorService`: a fresh install
renders the bundled defaults with no extraction step, and an admin drops
`movie.jinja` (no underscore) into `<data>/templates/channel-guide/` to
override it.

Intentional differences from ErsatzTV's own templates:

- `<rating>` splitting (the `/`-delimited multi-value `ContentRating`
  convention, `us:` prefix mapping to `MPAA`/`VCHIP`) is resolved in Rust
  before the template ever runs, since minijinja does not ship Scriban's
  `string.split`/`string.replace` filter pipeline; templates just loop a
  pre-built `{system, value}` list.
- Movie guide icons use thumbnail artwork when no poster exists. ErsatzTV's
  movie handler selects posters only; ErsatzRS deliberately degrades to the
  available image and retains its thumbnail sizing hint so EPG clients receive
  useful artwork instead of an empty icon slot.
- The bundled Remote Stream template emits persisted genres as `<category>`
  elements. ErsatzTV already exposes `RemoteStreamGenres` to custom templates
  but omits them from its bundled default; emitting them improves guide
  filtering and discovery without requiring every administrator to override
  the template.

The episode poster-image omission was retired on 2026-09-27 during UCU-08
row 124. Upstream `b3a35116` removes the trailing quote from that image URL;
the bundled ErsatzRS episode template now emits the valid poster image alongside
the show icon and episode still. Custom template precedence is unchanged.

## Consequences

Administrators can customize the guide exactly as in ErsatzTV — override one
per-kind template, leave the rest on the bundled default — using Jinja2
syntax instead of Scriban. Existing ErsatzTV custom templates need only
mechanical syntax translation (`{{ if x }}` → `{% if x %}`, `{{ end }}` →
`{% endif %}`/`{% endfor %}`) since context variable names match. A malformed
custom template cannot corrupt the whole guide response: each rendered
fragment is parsed and re-emitted through the same `quick_xml` writer that
builds the rest of the document, and a parse or template-evaluation failure
falls back to a minimal `<title>`-only element for that one channel/
programme rather than failing the request.

Rich content resolution preloads all distinct `MediaItem` ids and hydrates
their linked genres, artwork, GUIDs, and music fields through bounded batch
queries within a single guide request (a movie or episode scheduled many times
across the window is fetched once). Templates compile once per guide request
and are reused for every fragment; they are not cached across requests, which
keeps custom override changes visible on the next fetch.

## Alternatives considered

- Extend `music_video_credits.rs`'s hand-rolled Scriban-subset interpreter
  to cover the XMLTV templates. Rejected: that interpreter only recognizes a
  handful of hardcoded conditional strings from its own bundled templates,
  not a general parser: it cannot support arbitrary user-authored loops or
  conditionals over seven different, larger templates.
- `tera` instead of `minijinja`. Both are comparable, actively-maintained
  Jinja2-style engines. `minijinja` was chosen for its smaller dependency
  footprint (one small transitive crate) and syntax closer to what most
  Jinja2-experienced template authors already expect.
- Extract bundled defaults to disk at startup (ErsatzTV's own
  `ResourceExtractorService` pattern, already used elsewhere in this
  codebase for streaming resources/fonts). Rejected in favor of the
  `music_video_credits` convention (serve bundled defaults directly, no
  extraction) to avoid a filesystem round-trip and keep the bundled
  defaults always in sync with the running binary.

## References

- `$ERSATZTV_SRC/ErsatzTV/Resources/Templates/_movie.sbntxt` (and the six
  sibling `_*.sbntxt` templates)
- `$ERSATZTV_SRC/ErsatzTV.Application/Channels/Commands/RefreshChannelDataHandler.cs`
- `$ERSATZTV_SRC/ErsatzTV.Core/FileSystemLayout.cs`
  (`ChannelGuideTemplatesFolder`)
- `crates/ersatzrs-infra/src/xmltv_templates.rs`
- `crates/ersatzrs-infra/src/music_video_credits.rs` (sibling bundled-template
  convention this ADR follows)
