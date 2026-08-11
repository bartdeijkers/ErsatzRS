# Media source support

This page describes the end-user media-source workflows available in the
current beta.

| Source | Connection workflow | Library discovery | Item synchronization |
|---|---|---|---|
| Local folders | Complete in the admin UI | User creates each library | Complete for supported local media kinds |
| Jellyfin | Address and API key; the key is validated on save | Automatic after connection | Starts when selected libraries are saved |
| Plex | Plex account PIN sign-in discovers owned servers | Initial library discovery is not yet completed by the admin sign-in flow | Available after library rows exist |
| Emby | Address and API key can be stored | Initial library discovery is not yet completed by the admin connection flow | Available after library rows exist |

## Supported media kinds

Local libraries can be created for Movies, TV Shows, Music Videos, Other
Videos, Songs, Images, and Remote Streams. Provider libraries expose the media
kind reported by the provider and allow synchronization to be enabled per
library.

## Path replacements

Plex, Jellyfin, and Emby source rows expose a **Paths** editor. A replacement
maps the provider's file-path prefix to the prefix visible to ErsatzRS. It does
not move or copy media.

Path replacements are needed only when the provider path cannot be read as-is,
for example when provider and ErsatzRS containers mount the same host folder at
different paths.

## Credentials

Provider tokens and API keys are persisted as secret-bearing configuration.
Do not put them in URLs, screenshots, issue reports, or exported examples.
