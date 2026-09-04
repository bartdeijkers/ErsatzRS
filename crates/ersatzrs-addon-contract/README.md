# ErsatzRS add-on contract

Dependency-light Rust types for the versioned, out-of-process ErsatzRS add-on
protocol. The crate owns package and repository manifests, readiness and
operation diagnostics, the media-list and Remote Stream NDJSON records, and
the provider-neutral rich metadata document shared by all list/item editors.

See the [ErsatzRS add-on package reference][reference] for protocol details and
release compatibility.

## Declarative media-list options

Manifest schema v5 lets a package declare bounded Boolean and enum controls on
the `media-list.options.v1` capability. The capability identifier is also the
option schema version; packages must publish a new capability version rather
than changing the meaning of an existing schema.

```toml
manifest_version = 5

[[capabilities]]
id = "media-list.options.v1"

[[capabilities.options]]
key = "fixture_mode"
label = { "en-US" = "Fixture mode" }
description = { "en-US" = "Use the package's fixture behavior." }
scope = "both"
operation_targets = ["enrich", "playback"]
applicability = "all"
kind = "enum"
default = "none"
choices = [
  { value = "none", label = { "en-US" = "None" } },
  { value = "enabled", label = { "en-US" = "Enabled" } },
]
```

Options are declarative: the host renders and validates them, resolves list
defaults and item overrides, and sends only typed effective values through a
compatible operation contract. `media-list.options.v1` is not itself an
executable operation. Unknown schema versions, keys, enum values, types,
scopes, operations, applicability, and oversized envelopes fail closed.

Legacy manifests and frozen `media-list.import.v1` and
`remote-stream.play.v1`/`v2` requests remain unchanged. A manifest that has no
supported option capability gets no option controls and receives no option
values.

[reference]: https://github.com/bartdeijkers/ErsatzRS/blob/main/docs/reference/addon-packages.md
