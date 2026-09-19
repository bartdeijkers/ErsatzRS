# Preservation inventory

This maps the inspected page and relevant conditional source branches to their
proposed destination. It proves design traceability, not implemented parity.
Sample values are synthetic. Full item media-detail pages retain their existing
scope; this proposal does not replace that separate feature.

| Existing control or information | Proposed destination | Contract to retain |
| --- | --- | --- |
| List title / `metadata_title` | List settings, title; overview heading | Persistent label; existing metadata save and conflict handling. |
| List identity and inferred type | All views, identity; List settings | One canonical title value; current versus requested presentation. |
| Source URL | List settings, Source and updates | Preserve current read-only status; do not silently add URL editing. |
| Installed add-on and Change add-on | List settings, Source actions | Compatible choices; existing impact and confirmation. |
| Managed folder | List settings, Managed location | Read-only; no editable private filesystem path. |
| `refresh_cadence` | List settings, Source and updates | Adaptive, daily, weekly, monthly, yearly, never; unsupported saved Adaptive warning. |
| Last successful / next scheduled / retry refresh | Beside cadence | Truthful timestamps and retry eligibility. |
| `refresh_publication_policy` | Beside cadence | Review versus safe additions; unsafe changes stay in review. |
| `load_item_details` | Source and updates | Automatic versus manual detail policy; overview-first and controlled rate. |
| `generate_playlist` | Source and updates | Provider order and update after publication. |
| `delivery_mode` | Playback and access, Delivery | Stream, download ahead, new-only; cutoff and frozen current playback. |
| `presentation_kind` | List settings, List metadata | Auto plus all seven library kinds; unsupported/error states. |
| Current/requested library-type help | Beside collection type | Explain migration consequences before publication. |
| `metadata_original_title` | List metadata | One list form. |
| `metadata_sort_title` | List metadata | One list form. |
| `metadata_year` | List metadata | Numeric validation and submitted-value recovery. |
| `metadata_plot` | List metadata, Summary | Complete text retained. |
| `metadata_languages` | List metadata | Metadata only; not audio/subtitle selection. |
| `metadata_genres` | Genres, tags and credits disclosure | Existing encoding and save contract. |
| `metadata_tags` | Genres, tags and credits disclosure | All tags; one per line; retain count/help. |
| `metadata_studios` | Genres, tags and credits disclosure | Existing encoding. |
| `metadata_directors` | Genres, tags and credits disclosure | Existing encoding. |
| `metadata_writers` | Genres, tags and credits disclosure | Existing encoding. |
| `metadata_artists` | Genres, tags and credits disclosure | Existing encoding. |
| `metadata_people` | Genres, tags and credits disclosure | Full credits and existing line format. |
| `metadata_poster` | Artwork editor | URL value remains part of list save. |
| `metadata_fanart` | Artwork editor | URL value remains part of list save. |
| `metadata_thumbnail` | Artwork editor | URL value remains part of list save. |
| Artwork uploads and provider candidates | Artwork owning editing group | All three roles; ETag; native multipart adapter if missing. |
| Metadata conflicts / provenance | Beside affected field; item detail | Never suppress a conflict in a closed optional section. |
| Apply changes | The owning task group | Full payload and revision/ETag guarantees; grouped validation before success. |
| Publish list | Items, Publish changes | Exact saved revision; disabled reason; native POST. |
| Prepare publication | Readiness when required | Separate preparation; progress, retries and cancellation. |
| Readiness counts and blocker links | Items, overview and affected rows | Included, blockers, recommendations, optional detail counts. |
| Applyd revision and published snapshot | Items overview; activity | Distinct identifiers, not interchangeable counters. |
| Import progress / phase | Items status during active import | Exact counters, elapsed/phase information, timestamp; manual refresh. |
| Examined / archived / emitted / detail skipped counts | Import disclosure | No discarded counters; accurate meaning. |
| Pause / resume / retry import | Contextual active-import state | Server eligibility and backoff. |
| Test source and result | Activity, Source checks | Existing POST and complete bounded result. |
| Applyd draft change categories | Items filter and change disclosure | Added, metadata changed, excluded, unavailable, conflicted, pending removal, reappeared, removal candidate. |
| Reconcile entire source | Activity, Source checks | Non-destructive reconcile and two-observation removal contract. |
| Confirm removal | Affected item/change review | Explicit item ID and saved generation; preview retained. |
| Revision selectors / compare | Activity, Revision history | GET query state; list metadata/effective options/items comparison. |
| Retained/pinned revisions and restore eligibility | Revision history | Explain unavailable restore; do not show unavailable choices as usable. |
| Restore revision acknowledgement | Restore disclosure | Exact selected snapshot; replaces draft and publishes new revision. |
| Publication activity / jobs | Activity, latest outcome and details | Every job, event, timestamp, phase, outcome, and truncation notice. |
| Generation / correlation / attempts / related IDs | Operation details | Retain all diagnostic associations. |
| Download redacted diagnostics | Activity, Recovery and diagnostics | Existing redaction; artifact download is explicitly synthetic. |
| Recovery Retry / Restore | Contextual recovery state | Actual cause, retry time, valid actions, revision/generation fences. |
| Item rank / provider order | Compact item row | Stable order; preserve accessible numbering at all widths. |
| Thumbnail and title link | Compact item row | Same item destination; synthetic artwork only in prototype. |
| First-air date / source runtime | Aligned row columns, gap evidence and item review | Preserve unknown values and provider order; runtime already exists on the row DTO. Date gaps never classify a season or extra automatically. |
| Include in publication | Item review | Shared item Apply; excluded status remains visible in rows. |
| Season / episode / unnumbered / invalid | Inline paired row controls and item review | Explicit Unnumbered choice maps to the existing empty pair; retain legacy 0/0, season-zero specials, server pair validation, warnings, explicit Apply. |
| Start numbering here | Row command → focused in-list numbering state | Immediately available per row; preserve validated starting ID, scope, draft inputs and off-page counts. |
| Media kind | Item review and bulk edit | Auto plus episode/movie/music video/other video/song/image. |
| Primary feature / movie extra | Conditional movie classification | Existing collection role independent from media kind. |
| Playback behavior / provider value / override / conflict | Row summary and item review | Unknown/finite/live classification and conflict warnings. |
| Item Summary / `plot`, Genres / `genres`, Tags / `tags` | Item review, Summary, genres and tags disclosure | Editable fields share the item form; commas separate genres, lines separate tags. Apply/Discard and navigation retain the whole draft. |
| Item Chapters / `chapter_input` | Item review, Chapters disclosure | Editable times and titles; validate before applying any item fields, retain invalid input and reveal the error. Production preserves provider fallback, operator overrides, restore-provider and derived-item restrictions. |
| People / provenance snippets | Item media-information disclosure | Full content retained; summary may be shorter. |
| Availability reason / detail status / diagnostic | Row when consequential; item detail | Error not hidden behind an apparently healthy badge. |
| Request item details / retry | Item tools when eligible | Keep disabled complete state and bounded request semantics. |
| Local replacement search / selection / clear | Item tools, Local replacement | Retain result type/version, valid/invalid replacement, clear action. |
| Full Remote Stream/media details | Existing item media-detail destination | All video/audio/subtitle tracks, languages and diagnostic information. |
| Bulk scope / kind / liveness / apply / undo | Items, Bulk edit disclosure | All/current page/needs review; explicit impact before apply. |
| Numbering season / start / count / gap / overwrite | Focused in-list numbering state and Numbering rules | Preserve legacy algorithms, occupied-number skipping, missing-only count semantics, undated provider order and off-page preview; selected-ID and bounded-endpoint modes are explicit extensions. |
| Exact non-contiguous selection / season-only change | Native row checkboxes and checked-row actions | New proposal: leave trailers, bloopers, specials and other unselected items untouched; never conflate selection with publication inclusion. |
| Item filters / clear / range / page size / pager | Items, filter and footer | Preserve category/query/page; 10/25/50/100/all. One page in sample. |
| Provider options bulk scope | Items tools; Playback and access | All/page/filter; mixed and off-page values; preview/apply/undo. |
| Browser cookies | Playback and access, native default/override select | None/Firefox/Chrome/Chromium; service-account scope; no raw cookies. |
| Remove SponsorBlock segments | Playback and access, native default/override select | Finite-only; all delivery modes; prepare implications. |
| Effective values / package/list inheritance | Options summaries and item overrides | Distinguish inherited value from explicit equal-valued override. |
| Reset to package/list default | The owning option editor | Preserve correct reset target; no aggregate reset. |
| Provisional settings activation / undo | Contextual option status | Activation is not silently treated as an existing saved schema. |
| Access state / test / cancel / result | Item provider overrides and states | Revision-scoped queued/running/terminal result; valid cancellation. |
| Future schema-defined options | Same provider group | Dynamic fields remain discoverable; no provider-specific whitelist. |
| List `warning_gb` | Playback and access, Disk usage warning | 25 GB default, nonnegative, zero disables, independent native save. |
| Combined-list warning link/context | Disk usage warning help | Clearly global; does not become a per-list field. |
| Reset consequences and typed name | Activity, Reset and reload | Complete impact, name confirmation, queue result and recovery. |

## Details still requiring implementation decisions

The revised bulk-numbering proposal also adds optional minimum/maximum source
runtime bounds and explicit handling of unknown runtime. These filter numbering
candidates only; they do not change list visibility or publication inclusion.
See [episode-numbering.md](episode-numbering.md) for the filtering order, bounds,
occupied-number preservation and server validation requirements.

The inventory includes branches that were not active on the inspected list.
Synthetic impact, source-test, replacement, conflict and media-track views now
make those information destinations inspectable. Production must still reuse
the authoritative contracts, including schema activation/undo and runtime outcomes.
The prototype does not establish provider or persistence parity. The four item
metadata editors above are now interactive; the separate media-details example
remains a read-only information destination, not an editing substitute.

The revised prototype exercises synthetic native HTTP forms only. No production
endpoint or database operation was exercised. See [design.md](design.md) for
the revised task boundaries and concrete scenario destinations.


The current artifact labels editing actions Save; existing application commands
may still be named Apply. Native `cookie_policy` and `sponsor_policy` selectors
map to the existing value/override/reset model in the synthetic adapter. Default
and explicit equal-valued overrides remain distinct. Production schema discovery
and reset targets remain authoritative; the sample default values are synthetic.
