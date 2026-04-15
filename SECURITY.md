# Security Policy

ErsatzRS is pre-alpha. There are no supported production releases yet.

## Supported Versions

| Version | Supported |
| --- | --- |
| `main` | Best effort |
| Releases | Not available yet |

## Threat Model

ErsatzRS will expose HTTP endpoints, HLS streams, IPTV playlists, HDHomeRun
discovery on the local network, FFmpeg process execution, media-library
connectors, and database access. Security-sensitive areas include:

- Authentication and authorization for web and IPTV endpoints.
- Handling of media paths and generated FFmpeg arguments.
- Logs that may include file paths, media names, tokens, or upstream server URLs.
- Secrets for databases, Trakt, Plex, Jellyfin, Emby, and other integrations.
- UDP discovery behavior on trusted and untrusted networks.

## Reporting a Vulnerability

Use GitHub private vulnerability reporting when it is available for the
repository. If it is not available, contact the maintainer privately through the
GitHub profile.

Do not file public issues for vulnerabilities until a fix or disclosure plan is
agreed.

Please include:

- A clear description of the issue.
- Reproduction steps.
- Affected version or commit.
- Relevant logs with secrets removed.
- Any known mitigations.

## Secret Handling

Never paste real API keys, passwords, access tokens, cookies, or private media
library data into issues, pull requests, logs, snapshots, or fixtures.
