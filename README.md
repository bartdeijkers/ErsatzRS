# ErsatzRS

ErsatzRS turns your local, Plex, Jellyfin, or Emby media library into scheduled
TV channels. Watch them through IPTV clients using M3U playlists, XMLTV guides,
MPEG-TS or HLS streams, and HDHomeRun-compatible endpoints.

It is a from-scratch Rust reimplementation of
[ErsatzTV Legacy](https://github.com/ErsatzTV/legacy), with a web administration
interface, a bundled Next playback engine, and managed FFmpeg runtimes.

Read the [changelog](CHANGELOG.md) for release features, fixes, and breaking
changes.

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

ErsatzRS builds on ErsatzTV’s approach to personal television, with a Rust
implementation, broader language support, richer media organization, and
graphics rendered through FFmpeg. It preserves upstream behavior where
possible and records deliberate differences in architecture decision records
(ADRs).

## Install and start

Download an archive from [ErsatzRS releases](https://github.com/bartdeijkers/ErsatzRS/releases),
then follow the guide for your deployment. **The 1.2.0 release provides Windows x64 and
Linux x64, ARM64 and ARMv7 archives; its Docker image is deferred.**
The Linux archives require glibc 2.39 or newer.

| Deployment | Start here |
|---|---|
| Windows | [Install on Windows](docs/how-to/install-on-windows.md); extract the complete archive and run `ErsatzRS.exe` |
| Linux | [Install on Linux with systemd](docs/how-to/install-on-linux.md) |
| Docker | [Install with Docker](docs/how-to/install-with-docker.md), for a release that includes an image; deferred for 1.2.0 |

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
macOS builds are deferred. ARM packages have build and emulation coverage;
operation on native ARM hardware remains unverified.

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

Release 1.2.0 delivers Level 1/1.5 teletext authoring, preview, publication and
a TV-guide add-on. Level 2.5 remains **experimental**. The planned “The Box”
video-jukebox extension is not yet available.

See [project progress](docs/progress/README.md) for delivered work, remaining
gaps and deferred features.

## Differences compared to ErsatzTV

The main user-facing differences are listed below. Links lead to the decisions
behind them.

- **[More interface languages](docs/adr/0012-admin-ui-nl-localization.md).**
  The admin interface adds Dutch, French, German, Italian and Spanish, among
  others, alongside English, Polish and Brazilian Portuguese.
- **[Subtitle language defaults](docs/adr/0052-global-subtitle-language-preferences.md).**
  Set a preferred and fallback subtitle language once, while keeping channel
  and programme overrides.
- **[Richer tagging and metadata](docs/adr/0062-uniform-media-metadata-facets.md).**
  Use tags, genres, ratings, languages, studios and music labels across media
  types for search and smart collections.
  [Extras and alternate versions](docs/adr/0018-extras-alternate-versions-and-stream-roles.md)
  also retain their relationship to the original media.
- **[More control over programme selection](docs/adr/0019-similar-video-cooldown.md).**
  Smart Shuffle keeps similar programmes apart using metadata-based rules.
  A [dedicated smart-collection editor](docs/adr/0083-separate-search-discovery-from-smart-collection-composition.md)
  helps build and preview collection rules.
- **[Installable add-ons](docs/adr/0081-use-local-addon-repositories.md).**
  Extend discovery and content through local repositories or ZIP packages,
  with explicit installation and updates. Automatic official repository
  delivery is [planned](docs/adr/0118-bundle-and-refresh-official-addon-repository.md).
- **[FFmpeg-based graphics](docs/adr/0005-drawvg-graphics-engine.md).**
  Native graphics need no separate renderer and support
  [animated JPEG XL](docs/adr/0115-play-compact-jpeg-xl-graphics.md).
  Managed ErsatzRS-ffmpeg runtimes use
  [FFmpeg 9](docs/adr/0063-ffmpeg-9-runtime-and-profile-options.md).
- **[Teletext](docs/adr/0121-teletext-pages-transport-and-addon-contract.md).**
  Author and publish pages or use the TV-guide add-on. Level 1/1.5 is delivered;
  Level 2.5 remains experimental.
- **[Native playback remains available](docs/adr/0095-native-pipeline-instead-of-ersatztv-next-engine.md).**
  Next is bundled and selected for new channels. Existing channels keep their
  selected engine; profile capabilities differ between engines.
- **[SQLite-only storage](docs/adr/0080-standardize-on-sqlite.md).**
  No MySQL/MariaDB server is supported. Search also runs locally,
  [without Elasticsearch](docs/adr/0009-sql-backed-search-and-no-elasticsearch.md).
- **[Local diagnostic downloads](docs/adr/0129-retain-local-troubleshooting-results.md).**
  Download troubleshooting results and choose where to share them; there is no
  diagnostic upload service.

See [system requirements](docs/reference/system-requirements.md) for platform
availability and [project progress](docs/progress/README.md) for remaining
parity gaps.

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

Changes reach `main` through pull requests with the required `CI ok` check.
CI runs on self-hosted runners; workflows for pull requests from forks need
maintainer approval. See [CONTRIBUTING.md](CONTRIBUTING.md) for review and
verification requirements.

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
