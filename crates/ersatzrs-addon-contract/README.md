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

Operation targets include `enrich`, `artifact`, `preparation`, `playback`, and
`test_access`. Access checks use saved draft options. A Boolean option with
`applicability = "finite_only"` and a `preparation` target may additionally
declare `requires_preparation = true`: an effective true value requires
processing evidence before publication. This declaration is provider-neutral and applies at either
list-default or item-override scope. The flag defaults to false and is omitted
when false, preserving the serialized schema and digest of older declarations.
These declarations do not introduce an executable access or preparation protocol.

Legacy manifests and frozen `media-list.import.v1` and
`remote-stream.play.v1`/`v2` requests remain unchanged. A manifest that has no
supported option capability gets no option controls and receives no option
values.

## Interval metadata

`media-list.interval-metadata.v1` invokes `interval-metadata` with source and
provider identity, the requested tool version, a host context fingerprint, and
effective `preparation` options. It must not download or stage a media file.
The host binds saved source/chapter timing into the fingerprint separately;
the provider echoes the fingerprint and returns full-source timing.

The result echoes provider identity and tool version, with an `outcome` whose
`kind` is `ready`, `unsupported`, or `failed`. Ready requires a positive
`source_duration_milliseconds` and an explicit `excluded_intervals_milliseconds`
array of half-open `{start, end}` ranges. An empty array means successful
discovery with no exclusions; missing data must not imply success. Failed may
include `retry_after_seconds` between 1 and 3600. Responses are limited to
64 KiB and 1024 source intervals. No credentials, paths, freeform diagnostics
or playback transport URLs belong in the result.

## Retained playback

`remote-stream.play.v4` invokes `play-intervals` with one bounded JSON request on stdin.
Stdout carries a continuous NUT byte stream containing raw video and PCM audio,
not a JSON response or transport descriptor. Earlier playback versions remain
unchanged. The request and validation rules are defined in
[`remote_stream_retained_play.rs`](src/remote_stream_retained_play.rs).

The host supplies published source/provider identity, an opaque frozen context
fingerprint, whole-source duration, effective playback options, an optional
height ceiling, and ordered retained source intervals. Intervals are half-open
millisecond ranges, already clipped to the programme seek and remaining duration.
The selection must contain 1–1025 positive nonoverlapping ranges within the source;
the serialized request is limited to 512 KiB. Request and range objects reject
unknown fields. Deserializing callers must enforce the byte limit before parsing
and validate the request before dispatch.

The provider executes exactly that selection with a continuous zero-based edited
output clock. It must not rediscover exclusions, change the frozen duration, or
fall back to uncut media or a prepared file. Failed extraction or unsupported
media fails the invocation. The host retains scheduling and final output-clock
policy, startup deadlines, backpressure and cancellation ownership.

Media transfer stays provider-owned; extracted transport URLs, HTTP headers and
browser cookie bytes must not appear in metadata, logs or diagnostics. Request
Debug is redacted even before validation. The official yt-dlp provider must use
its shipped downloader capabilities for section transfer. This contract does not by
itself establish provider implementation or measured cut continuity.

[reference]: https://github.com/bartdeijkers/ErsatzRS/blob/main/docs/reference/addon-packages.md
