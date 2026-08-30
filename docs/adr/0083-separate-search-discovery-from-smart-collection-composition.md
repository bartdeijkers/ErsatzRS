# ADR 0083: Separate search discovery from smart collection composition

## Status

Accepted

## Implementation

Implemented (working tree, 2026-08-30)

## Context

ErsatzTV and the initial ErsatzRS port combine two related but different jobs on
the global `/search` route: finding media and persisting the current query as a
smart collection. The upstream workflow opens a modal that can create a new
smart collection or replace an existing collection query. The separate smart
collection editor exposes only the collection name; it does not help an
operator understand or safely change the stored Lucene-style query.

ErsatzRS already has one core-owned `SearchExpression` parser/evaluator and one
canonical query string shared by global search, smart collections, scheduling,
and Search actions. That contract must remain intact. The UI must not create a
second filter representation that can drift from the stored query.

The focused Variant 4 design study found that putting quick filters, a query
tree, a rule inspector, and save controls inside one nested search drawer makes
the common discovery task compete with durable collection authoring. It also
creates an unexpected mode in which quick controls become read-only after
nested logic is introduced.

This is a deliberate UX deviation from ErsatzTV. Search and smart-collection
persistence remain behaviorally compatible, but ErsatzRS gives complex
collection authoring its own route and structured editor.

## Decision

Keep global `/search` results-first.

- The main search field and result sections remain the dominant surface.
- A small native filter disclosure offers only common, shallow filters and
  reports its active-filter count when closed.
- Active quick filters are serialized into the same canonical query string
  used by every existing search consumer. Clearing filters does not require
  reopening the disclosure.
- Replace the search-page `Save As` modal with a `Create smart collection`
  navigation action. It carries the exact canonical query to
  `/media/smart-collections/add` and preserves a return link to the originating
  search.

Make the smart-collection add/edit route the dedicated composer.

- Collection identity, query rules, validation, preview information, and save
  actions share one task context.
- The default rule editor presents the top-level boolean model as three
  progressive-disclosure cards: `Match all` (`AND`), `Match any` (`OR`), and
  `Exclude any` (`NOT`).
- A shallow query stays in those cards. A genuinely nested child group opens in
  a focused editor that shows one group and its immediate children at a time,
  with an explicit return to the overview. Recursive indentation is not the
  default interaction.
- The canonical query remains visible as a review and compatibility surface.
  Structured and canonical views are projections of the same typed AST, not
  independently editable sources of truth.
- An operator can save the current advanced query under a unique name, recall
  it into the composer later, modify it, and update the same saved query. A
  saved search stores only its name and canonical query; it is deliberately a
  reusable authoring draft rather than a smart collection.
- Saving or recalling a reusable query always validates and projects it through
  the same core AST. Saving from a structured edit uses the current composer
  model, even when a previous asynchronous canonical projection has not yet
  completed.
- Reusable saved searches have no scheduling, cache-refresh, playout-refresh,
  or membership side effects. Those begin only when a smart collection is
  saved.
- Existing smart-collection persistence, validation, cache refresh, search
  target refresh, dependent playout refresh, and HTML POST fallback behavior
  remain unchanged.

Use native landmarks, headings, forms, fieldsets, buttons, links, and
`details`/`summary` disclosures where their semantics fit. The page wrapper
remains server rendered; only AST editing, focus-mode switching, and live
preview state belong in a narrow Leptos hydration island. At 320 CSS pixels the
composer regions stack without page-level horizontal scrolling.

The former nested drawer is retained only as design-history reference. It is
not a production fallback or a second advanced-search mode.

## Consequences

The common search path becomes easier to scan and keeps advanced collection
controls out of the result hierarchy. Operators can inspect boolean meaning in
bounded chunks before saving a durable collection, while existing raw queries
continue to round-trip through the canonical query contract.

The route handoff must preserve the query byte-for-byte until the operator
edits it. Browser history and the explicit return link must restore the same
search URL. Add and edit routes gain more UI and hydration responsibility than
the upstream name-only editor.

The typed search AST needs a stable formatting/projection API. Query forms that
cannot be represented by the shallow ALL/ANY/NONE overview must remain valid
and use focused nested editing rather than being flattened or silently
rewritten.

Reusable saved searches add an ErsatzRS-owned SQLite entity because ErsatzTV
does not have an equivalent named-query draft. Keeping that entity separate
from `SmartCollection` prevents an authoring convenience from accidentally
refreshing collection membership or dependent playouts. The HTML fallback
preserves the collection draft, saved-search identity, and exact originating
search URL across saved-search load/save and smart-collection validation
errors.

Localization expands for filter, group, rule, preview, focus-mode, and
validation labels in every supported culture. Focused browser proof is required
for Chromium and Firefox, including the route handoff, a shallow group edit, a
nested focus edit, saved-query create/recall/update, current-model persistence,
no-JavaScript save and return navigation, keyboard operation, and 320-pixel
reflow.

## Alternatives Considered

- Keep the ErsatzTV-style `Save As` modal: rejected because it persists an
  opaque query without providing a safe authoring or review context.
- Put quick filters and the complete nested editor in one `/search` drawer:
  rejected because simultaneous result, filter, tree, inspector, and save
  contexts exceed the intended cognitive budget.
- Make a query tree the default composer: rejected because shallow ALL/ANY/NONE
  queries do not need tree navigation or recursive indentation.
- Store structured filters separately from the canonical query: rejected
  because two persisted sources of truth can diverge across search,
  scheduling, and smart-collection consumers.
- Model reusable queries as incomplete smart collections: rejected because a
  saved authoring draft must not participate in membership, scheduling, or
  playout refresh behavior.
- Replace the canonical query with a new JSON filter language: rejected because
  it would break the ErsatzTV-compatible search and persistence contract.

## References

- [Variant 4 design study](../../plans/artifacts/search-collection-design-variants.html)
- [Admin UI visual language](../reference/ui/visual-language.md)
- [ADR 0009: SQL-backed search and no Elasticsearch backend](0009-sql-backed-search-and-no-elasticsearch.md)
- [ErsatzTV deviation index](../explanation/deviations.md)
- [`/search` implementation](../../crates/ersatzrs-web/src/admin/pages/search.rs)
- [Smart collection editor](../../crates/ersatzrs-web/src/admin/pages/media/smart_collections.rs)
- `$ERSATZTV_SRC/ErsatzTV/Pages/Search.razor`
- `$ERSATZTV_SRC/ErsatzTV/Shared/SaveAsSmartCollectionDialog.razor`
- `$ERSATZTV_SRC/ErsatzTV/Pages/SmartCollectionEditor.razor`
