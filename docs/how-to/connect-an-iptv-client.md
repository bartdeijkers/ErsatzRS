# Connect an IPTV client

Use this guide to add ErsatzRS channels and guide data to an IPTV application.

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

