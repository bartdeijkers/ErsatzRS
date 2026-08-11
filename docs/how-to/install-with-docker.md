# Install with Docker

Use this guide for a release tag that publishes the GHCR image. The current
Windows beta tags publish only `win-x64` archives; they do not publish a Docker
image.

When a normal release image is available, this procedure runs it with
persistent state and a read-only media mount.

## Prerequisites

- Docker Engine or Docker Desktop
- port `8409` available on the host
- internet access during the first startup for the managed FFmpeg runtime
- a media directory that the container can read

Normal-release images support `linux/amd64`, `linux/arm64`, and `linux/arm/v7`.

## Create the container

Create a named volume for the database, managed FFmpeg runtime, and persistent
application data:

```bash
docker volume create ersatzrs-data
```

Start ErsatzRS:

```bash
docker run --detach \
  --name ersatzrs \
  --publish 8409:8409 \
  --volume ersatzrs-data:/data \
  --volume /path/to/media:/media:ro \
  --restart unless-stopped \
  ghcr.io/bartdeijkers/ersatzrs:<tag>
```

Replace `/path/to/media` with the host directory that contains your media. Add
more `--volume` arguments when different libraries live in different places.
Use the container paths, such as `/media`, when configuring local libraries in
the admin UI.

Replace `<tag>` with a normal release tag that lists a Docker image in its
release notes. Prefer the immutable version tag over a moving channel tag.

## Wait for first startup

Follow the server log:

```bash
docker logs --follow ersatzrs
```

The first start downloads and validates the first-party FFmpeg 9 runtime when
no managed copy exists. Check readiness from the host:

```bash
curl --fail http://127.0.0.1:8409/healthz
```

Open <http://127.0.0.1:8409> after the probe succeeds.

## Use Compose instead

Create `compose.yaml` outside the repository:

```yaml
services:
  ersatzrs:
    image: ghcr.io/bartdeijkers/ersatzrs:<tag>
    container_name: ersatzrs
    restart: unless-stopped
    ports:
      - "8409:8409"
    volumes:
      - ersatzrs-data:/data
      - /path/to/media:/media:ro

volumes:
  ersatzrs-data:
```

Start it with:

```bash
docker compose up --detach
```

## Update the image

Pull and recreate the container while retaining the named data volume:

```bash
docker pull ghcr.io/bartdeijkers/ersatzrs:<tag>
docker stop ersatzrs
docker rm ersatzrs
```

Run the original `docker run` command again, or use Compose:

```bash
docker compose pull
docker compose up --detach
```

Back up the data volume before an update.

## Optional environment settings

Common operator overrides include:

```bash
--env RUST_LOG=info
--env LEPTOS_SITE_ADDR=0.0.0.0:8409
--env ERSATZRS_FFMPEG_RELEASE_TAG=<release-tag>
```

Do not set `ERSATZRS_FFMPEG_DOWNLOAD=0` unless a compliant FFmpeg and ffprobe
pair is already available. See the
[configuration reference](../reference/configuration.md) for precedence,
accepted names, and secret-bearing settings.

## Expose hardware acceleration

Hardware acceleration is optional. The container only advertises modes that
the runtime can probe successfully.

- For VAAPI or Intel QSV, pass the relevant `/dev/dri` devices.
- For NVIDIA, install and configure the NVIDIA Container Toolkit, then expose
  the GPU with Docker's GPU option.
- `linux/arm/v7` is CPU-only in the published image.

After changing device access, restart ErsatzRS and check **Support** ->
**Health Checks** before selecting hardware acceleration in an FFmpeg profile.

## Next steps

Return to the [documentation home](../README.md) to choose the next task. Use
container paths, rather than host paths, anywhere the admin UI asks for a media
location.
