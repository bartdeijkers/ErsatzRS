# Connect an IPTV client

Use this guide to add ErsatzRS channels and guide data to an IPTV application.

## Before you begin

At least one enabled channel must have a populated playout. The client device
must be able to reach TCP port `8409` on the ErsatzRS host. Test from the client
network rather than only from the server itself.

## Confirm the server address

From the client device, open:

```text
http://<ersatzrs-host>:8409/healthz
```

Use the ErsatzRS host name or LAN address. `127.0.0.1` and `localhost` work only
when the IPTV client runs on the same machine as ErsatzRS.

## Add the playlist and guide

Configure the client with:

| Purpose | URL |
|---|---|
| M3U channel playlist | `http://<ersatzrs-host>:8409/iptv/channels.m3u` |
| XMLTV guide | `http://<ersatzrs-host>:8409/iptv/xmltv.xml` |

Refresh both sources after adding or renumbering channels. The playlist embeds
the matching stream URLs and generated or uploaded channel logos.

## Configure a common client

Use the playlist and guide URLs above in the matching client concepts:

- **Jellyfin**: add an **M3U Tuner** under Live TV, then add an **XMLTV** guide
  data provider.
- **Kodi**: install and enable **PVR IPTV Simple Client**, set its M3U playlist
  URL, and set its EPG XMLTV URL.
- **TiviMate**: add an **M3U Playlist**, keep it as a TV playlist, then assign
  the XMLTV URL as its EPG if the guide is not detected automatically.
- **Channels DVR**: add a **Custom Channels** source with the M3U and XMLTV
  URLs. Choose a stream format that matches the generated playlist entries;
  do not force MPEG-TS when the included channels use `.m3u8` HLS streams.

Client labels can move between releases. Preserve the distinction between the
M3U channel source and XMLTV guide source even when a client groups both fields
in one setup screen.

## Confirm the import

The client should show the same channel numbers and names as **Channels**, load
programme rows from the XMLTV source, and start one direct channel test. If the
playlist imports but playback fails, test the channel URL below before changing
client settings. If guide rows are missing, fetch the XMLTV URL from the client
device and confirm that it contains the expected channel ID.

## Test a channel directly

Before debugging client import, open a channel in VLC or another player:

```text
http://<ersatzrs-host>:8409/iptv/channel/1.m3u8
```

For MPEG-TS:

```text
http://<ersatzrs-host>:8409/iptv/channel/1.ts
```

Replace `1` with the configured channel number.

## Use HDHomeRun discovery

Clients with HDHomeRun support can discover ErsatzRS on the local network. If
automatic discovery is unavailable across subnets or blocked by a firewall,
add the server manually using:

```text
http://<ersatzrs-host>:8409/discover.json
```

The related lineup URL is:

```text
http://<ersatzrs-host>:8409/lineup.json
```

HDHomeRun UDP discovery and HTTP access must be allowed through the host
firewall. Docker deployments also need host networking support appropriate to
the operating system for UDP discovery; the HTTP lineup URLs still work through
the published port.

## Fix guide or logo URLs that use the wrong host

ErsatzRS derives public URLs from the incoming request. Fetch the M3U and XMLTV
using the host name or address that clients should use. Avoid downloading the
playlist through `localhost` and then copying it to another device.

## Protect IPTV endpoints

IPTV access is open when no JWT signing key is configured. Setting
`JWT__IssuerSigningKey` enables HS256 JWT validation for IPTV routes. Clients
must then send a bearer token or add `access_token=<jwt>` to the playlist URL;
ErsatzRS propagates the query token into guide and stream URLs.

The token must contain an `exp` claim. Treat both the signing key and generated
tokens as secrets. See the [configuration reference](../reference/configuration.md).
