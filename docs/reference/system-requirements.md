# System requirements

This page lists the runtime requirements and currently published platforms for
ErsatzRS.

## Release status

The stable release line starts at v1.1.0. Back up persistent state before
updating and review the release notes for migration or packaging changes.

## Published packages

The v1.1.0 stable release publishes:

| Package | Platform |
|---|---|
| `ersatzrs-<tag>-linux-x64.tar.gz` | Linux x86-64 |
| `ersatzrs-<tag>-linux-arm64.tar.gz` | Linux AArch64 |
| `ersatzrs-<tag>-linux-arm.tar.gz` | Linux ARMv7 |
| `ersatzrs-<tag>-win-x64.zip` | Windows x86-64 |

No Docker image is included in v1.1.0. Existing GHCR tags must not be treated as
images of this release. Other releases may include images for Linux amd64,
arm64 and arm/v7; check their release notes before using them.

Beta tags may publish only the Windows archive. During the private-repository
period, the required matrix is Linux x64, ARM64, ARMv7 and Windows x64. macOS
x86-64 and Apple Silicon build, package, runtime and acceptance work is deferred
by maintainer decision (2026-10-26), not accepted. Existing code, artifacts and
evidence are retained without a verified macOS support claim. Resumption requires
explicit maintainer permission; no macOS builds or paid runners are authorized.

Native Linux ARM64/ARMv7 execution is also temporarily deferred by maintainer
decision (2026-10-26), because no hardware is available and no date is known.
ARM builds, packages, software/emulation checks and CI remain required. Native
compatibility is a provisional assumption, not verified hardware operation;
retain and resume native acceptance when suitable hardware becomes available.
This assumption is explicit in current Phase 9A acceptance.

ARMv7, ARM64, Intel macOS, and Apple Silicon macOS application acceptance for
Next, JavaScript audio selection, and chapter expressions remains explicitly
unverified. Building or publishing an archive does not establish that acceptance.

## Linux runtime baseline

The v1.1.0 Linux x64, ARM64 and ARMv7 application archives require glibc 2.39
or newer, such as Ubuntu 24.04 or Debian 13. Debian 12 (Bookworm), with glibc
2.36, cannot run these application archives. The managed FFmpeg runtime has
its own compatibility requirements; its minimum does not lower this baseline.

## Network requirements

| Requirement | Default |
|---|---|
| Admin, IPTV, XMLTV, and HDHomeRun HTTP | TCP port `8409` |
| HDHomeRun discovery | local-network UDP discovery |
| Managed FFmpeg acquisition | outbound HTTPS to GitHub Releases |
| Provider add-ons | outbound HTTPS to the hosts declared by each installed package |

Do not expose an unauthenticated admin UI directly to the public internet. Use
a trusted LAN, VPN, or an authenticated reverse proxy.

## Storage requirements

ErsatzRS needs writable persistent storage for:

- the SQLite database on a local filesystem with correct locking semantics;
- the managed FFmpeg runtime;
- extracted resources and, in builds containing the add-on host, verified
  add-on packages and add-on data; and
- temporary stream, subtitle, font, and transcode caches.

Media itself can be mounted read-only. The ErsatzRS process must be able to
traverse directories and read every configured media file.

Space requirements depend on the FFmpeg runtime, database size, artwork, and
concurrent HLS sessions. Keep enough free space for temporary transcode output
and runtime updates.

## FFmpeg requirement

Both FFmpeg and ffprobe must be on the supported FFmpeg 9.x runtime line and
expose the capabilities checked at startup. Older and unverified future major
versions are rejected. Required filters include `drawvg` for graphics and
`zscale` for color and HDR processing.

Official archives and Docker images do not bundle FFmpeg. On first startup,
ErsatzRS normally downloads the matching first-party runtime and stores it in
persistent state. An offline installation must provide a compliant explicit or
pre-placed runtime.

The v1.1.0 release pins checksum-verified ErsatzRS-ffmpeg v2.0.6 packages.

## Next requirement

New channels and standalone previews default to Next. Official archives and
Docker images include the pinned `next-0362692-ersatzrs.1` engine, its license,
and provenance. A source installation downloads and verifies that engine when
no compatible bundled or managed copy exists. Keep verified runtime files for
offline operation. Existing channels retain their selected playback engine,
and Native remains an explicit choice.

## Database support

- SQLite is the sole supported backend and requires no separate database
  server.
- The web host and scanner may share the database file on one host. Network
  filesystems and multiple active server instances sharing one database are
  unsupported.
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

Intel QSV, AMD AMF, and native Linux NVIDIA interop validation remains explicitly
unverified. Available software playback evidence does not prove these hardware
paths. Confirm the relevant host/device combination before relying on it.
