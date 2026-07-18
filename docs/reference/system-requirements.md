# System requirements

This page lists the runtime requirements and currently published platforms for
ErsatzRS.

## Release status

ErsatzRS is currently beta software. Back up persistent state before updating
and review the release notes for migration or packaging changes.

## Published packages

Normal releases currently publish:

| Package | Platform |
|---|---|
| `ersatzrs-<tag>-linux-x64.tar.gz` | Linux x86-64 |
| `ersatzrs-<tag>-linux-arm64.tar.gz` | Linux AArch64 |
| `ersatzrs-<tag>-linux-arm.tar.gz` | Linux ARMv7 |
| `ersatzrs-<tag>-win-x64.zip` | Windows x86-64 |
| `ghcr.io/bartdeijkers/ersatzrs:<tag>` | Linux amd64, arm64, and arm/v7 |

The current beta release line publishes only the Windows archive. Normal
release tags use the broader table above. macOS x86-64 and Apple Silicon remain
supported build targets, but the active release workflow does not currently
publish macOS archives.

## Network requirements

| Requirement | Default |
|---|---|
| Admin, IPTV, XMLTV, and HDHomeRun HTTP | TCP port `8409` |
| HDHomeRun discovery | local-network UDP discovery |
| Managed FFmpeg acquisition | outbound HTTPS to GitHub Releases |
| Provider and Trakt integrations | outbound HTTPS to the configured service |

Do not expose an unauthenticated admin UI directly to the public internet. Use
a trusted LAN, VPN, or an authenticated reverse proxy.

## Storage requirements

ErsatzRS needs writable persistent storage for:

- the SQLite database, unless MySQL or MariaDB is configured;
- the managed FFmpeg runtime;
- extracted resources and user-editable runtime scripts; and
- temporary stream, subtitle, font, and transcode caches.

Media itself can be mounted read-only. The ErsatzRS process must be able to
traverse directories and read every configured media file.

Space requirements depend on the FFmpeg runtime, database size, artwork, and
concurrent HLS sessions. Keep enough free space for temporary transcode output
and runtime updates.

## FFmpeg requirement

Both FFmpeg and ffprobe must be version 8.1 or newer and expose the capabilities
checked at startup. Required filters include `drawvg` for graphics and `zscale`
for color and HDR processing.

Official archives and Docker images do not bundle FFmpeg. On first startup,
ErsatzRS normally downloads the matching first-party runtime and stores it in
persistent state. An offline installation must provide a compliant explicit or
pre-placed runtime.

## Database support

- SQLite is the default and requires no separate database server.
- MySQL 8 and compatible MariaDB deployments require a binary built with the
  `mysql` feature and a `mysql://` or `mariadb://` database URL.
- Startup applies pending migrations and reports unknown, dirty, or mismatched
  migration history through health checks.

## Client support

Clients can consume:

- M3U playlists;
- XMLTV guide data;
- HLS channel streams;
- MPEG-TS channel streams; and
- HDHomeRun-compatible discovery and lineup endpoints.

Client codec and container support determines whether a channel can direct
stream or requires a compatible FFmpeg profile.

## Hardware acceleration

Hardware acceleration is optional. ErsatzRS supports the hardware modes exposed
by its FFmpeg profiles and runtime probes, including platform-specific NVIDIA,
VAAPI, QSV, AMF, VideoToolbox, RKMPP, and V4L2M2M paths where available.

The host must supply devices and drivers. Published `linux/arm/v7` Docker images
are CPU-only for this release line.
