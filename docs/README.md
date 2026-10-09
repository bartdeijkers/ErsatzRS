# ErsatzRS documentation

ErsatzRS turns media from local folders, Plex, Jellyfin, or Emby into linear
channels for IPTV, HLS, and HDHomeRun-compatible clients.

The stable release line starts at v1.1.0. Back up persistent state before every
update, and use a trusted LAN, VPN, or authenticated reverse proxy for the admin UI.

## New to ErsatzRS?

Follow [Build your first channel](tutorials/getting-started.md). It uses the
Windows release archive and leads from first startup to a
playable local-media channel.

Already know the basic workflow? Choose the result you need below.

## Install and update

- [Install on Windows](how-to/install-on-windows.md)
- [Install on Linux with systemd](how-to/install-on-linux.md) for a native
  service installed from a normal release archive
- [Install with Docker](how-to/install-with-docker.md)
- [Back up and restore ErsatzRS](how-to/back-up-and-restore.md)

Before installing, check the [system requirements](reference/system-requirements.md)
and [release bundle reference](reference/release-bundle.md).

## Add media

- [Add and scan a local library](how-to/add-and-scan-a-local-library.md)
- [Add Remote Streams](how-to/add-remote-streams.md)
- [Remote Stream playlist manifest reference](reference/remote-stream-playlist-manifests.md)
- [Connect a Jellyfin server](how-to/connect-a-jellyfin-server.md)
- [Media source support](reference/media-source-support.md)
- [JavaScript audio selectors](reference/javascript-audio-selectors.md)

## Extend ErsatzRS with add-ons

Install add-ons from an operator-managed checkout or a complete offline
repository bundle. Check the [add-on reference](reference/addons.md) for
package requirements and the separately installed tools each add-on needs.

- [Install and manage an add-on](how-to/manage-addons.md)
- [Understand the add-on trust boundary](explanation/addon-trust-boundary.md)

## Build and watch channels

- [Build a manual collection from media results](how-to/build-a-manual-collection.md)
- [Keep channel content updated with a smart collection](how-to/keep-channel-content-updated-with-a-smart-collection.md)
- [Build a channel from a collection](how-to/build-a-channel-from-a-collection.md)
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
- [All reference documentation](reference/README.md)
- [Why ErsatzRS manages FFmpeg](explanation/managed-ffmpeg-runtime.md)
- [Why copied portrait video loses orientation](explanation/portrait-video-and-mpeg-ts.md)

For a developer and operator view of the running system, see the
[current runtime architecture](explanation/runtime-architecture.md).

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

Architecture decisions live separately in the [ADR gateway](adr/README.md).
