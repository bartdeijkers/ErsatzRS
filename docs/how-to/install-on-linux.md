# Install on Linux with systemd

Use this guide to install a published Linux archive as a system service. This
follows Jellyfin's Linux packaging model: systemd owns startup at boot and the
server runs as a dedicated account. The **Settings** -> **UI** Windows-startup
checkbox is intentionally not shown on Linux.

## Choose the archive

Download the archive for the host architecture from the ErsatzRS release:

| Architecture | Archive suffix |
|---|---|
| x86-64 | `linux-x64.tar.gz` |
| AArch64 | `linux-arm64.tar.gz` |
| ARMv7 | `linux-arm.tar.gz` |

Beta tags may publish only a Windows archive. Use a normal release tag for the
Linux archives.

## Install the release files

Create a service account and stable application directories:

```bash
sudo useradd --system --home-dir /var/lib/ersatzrs --create-home \
  --shell /usr/sbin/nologin ersatzrs
sudo install -d -o ersatzrs -g ersatzrs /var/lib/ersatzrs/.ersatzrs
sudo install -d -o root -g root /opt/ersatzrs
```

Extract the downloaded archive. Copy the contents of its versioned top-level
folder into `/opt/ersatzrs`, keeping `ersatzrs-web`, `site/`, and `assets/`
together. Then set executable and read permissions:

```bash
sudo chown -R root:root /opt/ersatzrs
sudo chmod 0755 /opt/ersatzrs/ersatzrs-web
sudo find /opt/ersatzrs/site /opt/ersatzrs/assets -type d -exec chmod 0755 {} \;
sudo find /opt/ersatzrs/site /opt/ersatzrs/assets -type f -exec chmod 0644 {} \;
```

The service account also needs read access to every configured media folder.
Grant that access through an existing media group instead of making private
media world-readable. Add `ersatzrs` to the host's `video` or `render` group
when hardware acceleration requires those devices.

## Create the systemd unit

Create `/etc/systemd/system/ersatzrs.service` with:

```ini
[Unit]
Description=ErsatzRS media server
Wants=network-online.target
After=network-online.target

[Service]
Type=simple
User=ersatzrs
Group=ersatzrs
WorkingDirectory=/opt/ersatzrs
Environment=HOME=/var/lib/ersatzrs
Environment=DATABASE_URL=sqlite:///var/lib/ersatzrs/.ersatzrs/ErsatzRS.db
Environment=LEPTOS_SITE_ROOT=/opt/ersatzrs/site
Environment=LEPTOS_SITE_PKG_DIR=pkg
Environment=LEPTOS_OUTPUT_NAME=ersatzrs-web
ExecStart=/opt/ersatzrs/ersatzrs-web
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

The explicit `HOME` keeps the database and managed FFmpeg runtime under the
service account's persistent state directory. The site variables keep the SSR
binary and hydration bundle on the same release.

## Enable and verify the service

Reload systemd, enable startup at boot, and start ErsatzRS now:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now ersatzrs.service
sudo systemctl status ersatzrs.service
curl --fail http://127.0.0.1:8409/healthz
```

The first start can take longer while the managed FFmpeg 8.1 runtime is
downloaded and validated. Follow startup with:

```bash
sudo journalctl --unit ersatzrs.service --follow
```

Open `http://<server-address>:8409` after the health check succeeds. Do not
expose the unauthenticated admin UI directly to the public internet.

## Update or disable automatic startup

Stop the service, replace the complete `/opt/ersatzrs` release tree, and start
it again. Do not mix `site/` assets from different releases.

```bash
sudo systemctl stop ersatzrs.service
# Replace the release files under /opt/ersatzrs, then restore the permissions above.
sudo systemctl start ersatzrs.service
```

Disable boot startup without deleting the installation:

```bash
sudo systemctl disable --now ersatzrs.service
```

The systemd setting is machine-wide and requires administrator privileges, so
it is deliberately managed by the operating system rather than by a checkbox
in the web UI. See [ADR 0059](../adr/0059-platform-owned-automatic-startup.md)
for the platform boundary.
