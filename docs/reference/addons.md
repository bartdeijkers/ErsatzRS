# Add-on platform reference

The tiered-trust add-on host is implemented in the current source tree. The platform
remains in development until the official repository is published and its
production public key is pinned; existing beta archives do not gain add-on
support retroactively.

## Supported capabilities

| Capability | Host behavior |
|---|---|
| `addon.check.v1` | Required for every executable package; reports readiness. |
| `remote-stream.list.v1` | Enumerates bounded Remote Stream playlist entries. |
| `remote-stream.list.v2` | Adds stable provider identity, availability, content kind, and rich sidecar metadata. |
| `remote-stream.list.v3` | Adds duration, artwork, and structured fragment parent/bounds fields. |
| `remote-stream.list.v4` | Adds provider-neutral `unknown`, `finite`, or `live` playback behavior. |
| `remote-stream.item.v1` | Enriches only retained full entries after host selection. |
| `remote-stream.item.v2` | Adds bounded provider-supplied timestamp/title chapter text. |
| `remote-stream.play.v1` | Produces media bytes for one Remote Stream URL. |
| `media-list.list.v1` | Enumerates a bounded provider list for local matching and optional playlist generation. |
| `media-list.list.v2` | Adds explicit item availability and provider-owned content classification. |
| `media-list.list.v3` | Adds duration, artwork, and structured fragment parent/bounds fields. |
| `media-list.list.v4` | Adds one strict rich metadata document for list/item review, song and image classification, and explicit primary/extra collection roles. |
| `media-list.list.v5` | Adds provider-neutral Remote Stream liveness without changing non-Remote-Stream records. |
| `media-list.import.v1` | Pages overview discovery into durable host checkpoints and enriches exactly one committed item per bounded invocation. A package also declares the `media-list.list.vN` record schema it emits. |
| `declarative-panel.v1` | Renders host-owned text, status, key/value, and link blocks. |

Unknown capability versions are never executed. The v1 platform does not
provide a public add-on HTTP API, event stream, teletext capability, or
video-jukebox request API.

## Reference set

- [Package manifest, layout, permissions, and limits](addon-packages.md)
- [Repository trust, catalog, allowlist, and refresh](addon-repositories.md)
- [Invocation, environment, persistence, and lifecycle](addon-runtime.md)
- [Add-on media-list NDJSON contract](addon-media-lists.md)
- [Remote Stream playlist and NDJSON contracts](remote-stream-playlist-manifests.md)
