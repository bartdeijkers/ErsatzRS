# Install on Windows

Use this guide to install a published `win-x64` ErsatzRS archive and run it from
the system tray.

## Download and extract the archive

1. Open [ErsatzRS releases](https://github.com/bartdeijkers/ErsatzRS/releases).
2. Download `ersatzrs-<tag>-win-x64.zip`.
3. Extract the complete top-level folder to a stable, user-writable location.
4. Keep `ErsatzRS.exe`, `ersatzrs-web.exe`, `site/`, `public/`, and `assets/`
   together. Do not run a binary directly from inside the zip.

If Microsoft Defender SmartScreen appears, verify that the archive came from
the official ErsatzRS release page before continuing. Windows signing is
release-dependent; never bypass the warning for an archive from another
source.

Beta tags may publish only the Windows archive. Normal release tags publish the
active Linux and Windows archives plus the Docker image.

## Start ErsatzRS

Run `ErsatzRS.exe`. The launcher:

- starts `ersatzrs-web.exe` without a console window;
- adds an ErsatzRS icon to the notification area;
- provides **Open Web UI**, **Show Logs**, and **Exit** actions; and
- points the server at the bundled browser assets.

The first startup can take longer while ErsatzRS downloads and validates the
managed FFmpeg 8.1 runtime. On Windows, the server also copies the selected
runtime files next to the executable so future launcher starts resolve the same
pair. Use **Show Logs** to follow that work. Open <http://localhost:8409> after
the log reports that the server is ready, or after the readiness request in the
[first-channel tutorial](../tutorials/getting-started.md) returns `200`.

If Windows Defender Firewall prompts for access, allow the network profiles on
which IPTV clients should reach the server. Do not expose port `8409` directly
to the public internet.

## Stop ErsatzRS safely

Open the tray menu and select **Exit**. The launcher asks the server to drain
background work and close the database before it exits.

Do not end only the launcher process in Task Manager: that can leave the server
running in the background.

## Find user data and logs

Most mutable state is stored outside the release folder:

| Data | Default location |
|---|---|
| SQLite database and managed FFmpeg state | `%USERPROFILE%\.ersatzrs` |
| Logs, caches, and user-editable runtime scripts | `%LOCALAPPDATA%\ErsatzRS` |

The release folder can also gain co-located FFmpeg runtime files after startup.
Those files are replaceable and do not need to be included in a user-data
backup.

Use **Show Logs** from the tray menu to open the log directory. Back up the
state directory before replacing an installation; see
[Back up and restore ErsatzRS](back-up-and-restore.md).

## Update ErsatzRS

1. Back up the current data.
2. Select **Exit** from the tray menu.
3. Download and extract the new archive into a new folder.
4. Run the new folder's `ErsatzRS.exe`.
5. Check **Support** -> **Health Checks**.
6. Remove the old release folder only after the new version is healthy.

Do not copy an old `site/` or `public/` directory into a new release. The server
binary and browser bundle must come from the same archive.

## Diagnose a startup failure

The launcher shows a dialog when it cannot start the server or create the tray
icon. For other failures:

1. Open the log directory from the tray menu.
2. Inspect `ErsatzRS.log`.
3. Check whether another process already uses port `8409`.
4. Confirm the extracted `site/pkg`, `public`, and `assets` directories are
   present.
5. Follow [Troubleshoot startup and playback](troubleshoot-startup-and-playback.md).
