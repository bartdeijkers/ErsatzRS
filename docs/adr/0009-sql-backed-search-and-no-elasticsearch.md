# ADR 0009: SQL-backed search and no Elasticsearch backend

## Status

Accepted

## Implementation

Implemented — search is SQL-backed (feature `search-index`); no Elasticsearch backend exists.

## Context

ErsatzTV supports a Lucene search index and previously carried an experimental
Elasticsearch backend. ErsatzRS needs the same non-web search behavior for
scheduling, smart collections, Trakt list handoffs, and admin search without
adding a second service to the single-binary deployment target.

The Phase 7 Slice 19 search hardening pass added the remaining parity behavior
needed by the current non-web search surface: metadata-backed Director, Writer,
chapter count, duration, stream, provider GUID, and container expansion fields;
Lucene-style lowercasing, accent folding, fuzzy terms, phrase slop, boost
syntax consumption, numeric range handling, and jump-letter page maps.

ErsatzTV's Lucene analyzer does not use language stemming for the indexed media
fields. ErsatzRS therefore does not add a stemming dependency for this slice.

## Decision

Keep ErsatzRS search in process and SQL-backed for the parity target. The Rust
core owns the Lucene-like parser/evaluator, while infrastructure projects rows
from the normalized ErsatzTV-compatible schema into search documents.

Do not port the Elasticsearch backend. Elasticsearch is outside the ErsatzRS
single-static-binary deployment goal and was experimental in ErsatzTV. Tantivy
or another embedded inverted index may still be introduced later as an internal
optimization, but it is not required for parity and must preserve the current
core search contracts.

## Consequences

Operators do not need to deploy or administer Elasticsearch for ErsatzRS.

Search rebuild work remains a database projection rather than an external index
bootstrap step. This is simpler to operate, but broad searches still scan the
projected SQL document set in process until a future embedded index optimization
is justified by benchmarks.

Elasticsearch-specific settings, health checks, and troubleshooting guidance
are deliberately omitted from ErsatzRS. Documentation and parity tracking should
point to this ADR when explaining the dropped backend.

## Alternatives Considered

- Port Elasticsearch: rejected because it conflicts with the single-binary
  deployment model and adds operational cost for an experimental ErsatzTV path.
- Require Tantivy now: rejected because it is an implementation optimization,
  not a behavior requirement for the remaining Phase 7 parity work.
- Keep only ad hoc SQL string matching: rejected because ErsatzTV-compatible
  fuzzy, proximity, numeric, and page-map behavior belongs in the core search
  contract.

## References

- [Project contracts and status](../../plans/INDEX.md)
- [Current phase status](../../STATUS.md)
- [Parity matrix](../reference/parity-matrix.md)
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Search/LuceneSearchIndex.cs`
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Search/SearchQueryParser.cs`
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Search/CustomMultiFieldQueryParser.cs`
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Search/ElasticSearchIndex.cs`
