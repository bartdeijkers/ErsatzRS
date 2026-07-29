# ErsatzRS documentation

ErsatzRS turns media from local folders, Plex, Jellyfin, or Emby into linear
channels for IPTV, HLS, and HDHomeRun-compatible clients.

ErsatzRS is beta software. Back up persistent state before every update, and
do not expose the unauthenticated admin UI directly to the public internet.

## New to ErsatzRS?

Follow [Build your first channel](tutorials/getting-started.md). It uses the
currently published Windows beta archive and leads from first startup to a
playable local-media channel.

Already know the basic workflow? Choose the result you need below.

## Install and update

- [Install on Windows](how-to/install-on-windows.md)
- [Install with Docker](how-to/install-with-docker.md) when a normal release
  publishes a container image
- [Back up and restore ErsatzRS](how-to/back-up-and-restore.md)

Before installing, check the [system requirements](reference/system-requirements.md)
and [release bundle reference](reference/release-bundle.md).

## Add media

- [Add and scan a local library](how-to/add-and-scan-a-local-library.md)
- [Add Remote Streams](how-to/add-remote-streams.md)
- [Connect a Jellyfin server](how-to/connect-a-jellyfin-server.md)
- [Media source support](reference/media-source-support.md)

## Build and watch channels

- [Connect an IPTV client](how-to/connect-an-iptv-client.md)
- [Schedule extras and alternate versions](how-to/schedule-content-variants.md)
- [Admin navigation](reference/admin-navigation.md)
- [HTTP endpoints](reference/http-endpoints.md)

To understand how sources, libraries, schedules, playouts, and channels fit
together, read [How ErsatzRS builds a channel](explanation/how-ersatzrs-builds-a-channel.md).

## Operate and troubleshoot

- [Troubleshoot startup and playback](how-to/troubleshoot-startup-and-playback.md)
- [Configuration](reference/configuration.md)
- [Logging](reference/logging.md)
- [Glossary](reference/glossary.md)
- [Why ErsatzRS manages FFmpeg](explanation/managed-ffmpeg-runtime.md)

## Contributors and maintainers

Development, parity, architecture, test, and release documentation has a
separate [contributor documentation map](CONTRIBUTING.md#contributor-documentation-map),
so the end-user path above stays focused on operating ErsatzRS.

## How this documentation is organized

This documentation follows [Diátaxis](https://diataxis.fr/):

- tutorials provide a guided learning experience;
- how-to guides help accomplish a specific goal;
- reference pages provide facts and interfaces for lookup; and
- explanation pages build understanding of concepts and tradeoffs.

Architecture decisions live separately in [ADRs](adr/).
