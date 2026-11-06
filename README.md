# ErsatzRS

ErsatzRS turns your local, Plex, Jellyfin, or Emby media library into scheduled
TV channels. Watch them through IPTV clients using M3U playlists, XMLTV guides,
MPEG-TS or HLS streams, and HDHomeRun-compatible endpoints.

It is a from-scratch Rust reimplementation of
[ErsatzTV Legacy](https://github.com/ErsatzTV/legacy), with a web administration
interface, a bundled Next playback engine, and managed FFmpeg runtimes.

The stable release line starts at **v1.1.0**. Read the
[changelog](CHANGELOG.md#110---2026-10-09) for features, fixes, and breaking changes.

## What you can do

- Build channels from local libraries or connected media servers, using manual
  and smart collections, schedules, and playouts.
- Choose alternate audio and subtitles, including language preferences and
  JavaScript audio selectors.
- Use Next for new channels and playback previews, or explicitly choose the
  Native engine. Existing channels retain their selected engine.
- Add channel logos, watermarks, and broadcast graphics. Native graphics use
  FFmpeg's `drawvg` filters without a separate graphics renderer.
- Configure compatible playback profiles, including SDR/HDR output and
  hardware acceleration where the host and drivers support it.
- Inspect health checks and playback diagnostics, and extend media discovery
  with separately installed add-ons.

## Why ErsatzRS?

- ErsatzTV was archived (and later unarchived).
- I want to improve my a.i. assisted coding skills and the rust language. I learned a lot more

## Install and start

Download an archive from [ErsatzRS releases](https://github.com/bartdeijkers/ErsatzRS/releases),
then follow the guide for your deployment. **v1.1.0 includes Windows x64 and
Linux x64, ARM64 and ARMv7 archives; it does not include a Docker image.**
The Linux archives require glibc 2.39 or newer.

| Deployment | Start here |
|---|---|
| Windows | [Install on Windows](docs/how-to/install-on-windows.md); extract the complete archive and run `ErsatzRS.exe` |
| Linux | [Install on Linux with systemd](docs/how-to/install-on-linux.md) |
| Docker | [Install with Docker](docs/how-to/install-with-docker.md), for a release that includes an image; unavailable for v1.1.0 |

The release includes its pinned Next engine. On first startup, ErsatzRS
downloads and checksum-verifies its matching first-party FFmpeg runtime.
Allow internet access for that acquisition, or prepare the documented offline
runtime. Keep the server, `site/`, `assets/`, and Next files from the same archive
together; the browser interface needs the matching JavaScript and WASM assets.

Once startup finishes, open <http://localhost:8409>. Follow
[Build your first channel](docs/tutorials/getting-started.md) to scan a local
library and create a playable channel.

See [system requirements](docs/reference/system-requirements.md) for the package
matrix, runtime requirements, and unverified platform/hardware combinations.
macOS code compatibility is retained, but build, package, runtime and acceptance
work is deferred by maintainer decision. Making the repository public did not
lift that deferral; explicit permission is required to resume, and no verified
macOS support is claimed.

## Operate your server

- [Add and scan a local library](docs/how-to/add-and-scan-a-local-library.md)
- [Connect a Jellyfin server](docs/how-to/connect-a-jellyfin-server.md)
- [Connect an IPTV client](docs/how-to/connect-an-iptv-client.md)
- [Install and manage an add-on](docs/how-to/manage-addons.md)
- [Back up and restore ErsatzRS](docs/how-to/back-up-and-restore.md)
- [Troubleshoot startup and playback](docs/how-to/troubleshoot-startup-and-playback.md)

Keep SQLite state on a local filesystem and back it up before updating. Network
shares and multiple active servers sharing one database are unsupported. Use a
trusted LAN, VPN, or authenticated reverse proxy for administration.

The [documentation home](docs/README.md) brings together tutorials, task guides,
reference material, and explanations.

## Project status

Teletext is delivered since release 1.2.0: Level 1/1.5 authoring,
preview, explicit publication and the teletext TV guide are accepted. Level 2.5
is available as **experimental**; its enhanced-only acceptance is deferred. See
the [teletext design interchange reference](docs/reference/teletext-design-interchange.md)
and the teletext ADRs (0120-0124 and 0126) in [docs/adr/](docs/adr/). The "The Box"
video-jukebox extension (Phase 10) is planned work and is not advertised as a
feature.

Upstream Catch Up (UCU) with ErsatzTV has priority, but expect delays because time
and resources are always limited.

## Differences compared to ErsatzTV

- No MacOS builds yet
- No Docker builds yet, but planned for future releases
- Mysql support was dropped, because of maturity level SQLite
- Probably not completely backwards compatible with ErsatzTV profiles (untested)
- Add-ons are supported, but is still very much work in progress
- Teletext 1/1.5/2.5 engine added, also very much still work in progress
- Custom ERsatzRS-ffmpeg to enable jpeg-xl, drawvg
- 

## Develop and contribute

- Rust stable, pinned through [rust-toolchain.toml](rust-toolchain.toml).
- Network access for the first managed FFmpeg runtime download, or an explicit
  FFmpeg 9.x override/pre-placed managed runtime for offline development.
- SQLite as the sole supported database backend; no separate database server is
  required.
- For interactive UI development, use the `./run.sh` entry point: `./run.sh dev`
  for hot reload or `./run.sh build` for a build with matching WASM hydration
  assets. On Windows use the [run.cmd](run.cmd) shim. See [BUILD.md](BUILD.md)
  for tool installation and the full command reference.

Read [CONTRIBUTING.md](CONTRIBUTING.md) before making changes. The
[architecture](ARCHITECTURE.md), [porting workflow](PORTING.md),
[coding style](STYLE.md), [test contract](TEST.md), and
[architectural decisions](docs/adr/) describe the project conventions.

Changes reach `main` only through pull requests. `main` requires the single
`CI ok` check, which passes only when every CI job passed; pull requests that
touch only documentation get it within a minute. CI runs on the maintainer's
self-hosted runners, so workflows for pull requests from forks start only
after the maintainer approves them, and a full run can take several hours.

## Credits

ErsatzRS is a Rust port of [ErsatzTV Legacy](https://github.com/ErsatzTV/legacy) by
Jason Dove. ErsatzTV is the original work this project ports from, and ErsatzRS
would not exist without it. If you find either project useful, please consider
supporting Jason's continued work on the upstream:
[paypal.me/jasongdove](https://www.paypal.me/jasongdove).

## License

ErsatzRS is licensed under the zlib license, the same license as the upstream
ErsatzTV project. See [LICENSE](LICENSE) for the full text and the preserved
upstream copyright notice.
