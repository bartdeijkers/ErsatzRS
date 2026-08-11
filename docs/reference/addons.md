# Add-on platform reference

The signed add-on host is implemented in the current source tree. The platform
remains in development until the official repository is published and its
production public key is pinned; existing beta archives do not gain add-on
support retroactively.

## Supported v1 capabilities

| Capability | Host behavior |
|---|---|
| `addon.check.v1` | Required for every executable package; reports readiness. |
| `remote-stream.list.v1` | Enumerates bounded Remote Stream playlist entries. |
| `remote-stream.play.v1` | Produces media bytes for one Remote Stream URL. |
| `declarative-panel.v1` | Renders host-owned text, status, key/value, and link blocks. |

Unknown capability versions are never executed. The v1 platform does not
provide a public add-on HTTP API, event stream, teletext capability, or
video-jukebox request API.

## Reference set

- [Package manifest, layout, permissions, and limits](addon-packages.md)
- [Repository trust, catalog, allowlist, and refresh](addon-repositories.md)
- [Invocation, environment, persistence, and lifecycle](addon-runtime.md)
- [Remote Stream playlist and NDJSON contracts](remote-stream-playlist-manifests.md)
