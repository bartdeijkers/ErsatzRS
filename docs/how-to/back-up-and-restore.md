# Back up and restore ErsatzRS

Use this guide before upgrades, host migrations, or database experiments.

## Identify the database and state paths

Before stopping ErsatzRS, determine the active database from
`ErsatzRS_DATABASE__URL`, `ERSATZRS_DATABASE__URL`, or `DATABASE_URL`. When none
is set, the default SQLite database lives in the application state directory.
See the [configuration reference](../reference/configuration.md#startup-database-url)
for precedence and defaults.

Back up a custom SQLite database at the path in its URL. For MySQL or MariaDB,
make a database-native consistent backup while ErsatzRS is stopped; copying an
application directory does not back up that database.

## What else to back up

The application state directory contains managed runtime and add-on state in
addition to the default SQLite database. Logs and transcode caches are not
required for a restore.

| Installation | Essential state |
|---|---|
| Docker | the volume mounted at `/data` |
| Native Linux | `$HOME/.ersatzrs`; `/var/lib/ersatzrs/.ersatzrs` for the documented systemd service |
| Windows archive | `%USERPROFILE%\.ersatzrs` |

Back up `%LOCALAPPDATA%\ErsatzRS\scripts` on Windows, or the equivalent local
data `scripts` directory on another platform, if you customized MPEG-TS or
other runtime scripts.

Also copy deployment configuration, trusted-repository TOML, referenced secret
files, and Remote Stream or library definition files stored outside application
state. Environment-variable secret values must be restored through the
deployment secret store, not written into the backup archive.

## Back up a Docker volume

Stop the server so the database and WAL are closed consistently:

```bash
docker stop ersatzrs
```

Archive the named volume into the current directory:

```bash
docker run --rm \
  --volume ersatzrs-data:/source:ro \
  --volume "$PWD":/backup \
  alpine \
  tar -czf /backup/ersatzrs-data-backup.tar.gz -C /source .
```

Restart the server:

```bash
docker start ersatzrs
```

## Restore a Docker backup

Restore into a new volume first so the existing backup remains recoverable:

```bash
docker volume create ersatzrs-data-restored
docker run --rm \
  --volume ersatzrs-data-restored:/restore \
  --volume "$PWD":/backup:ro \
  alpine \
  tar -xzf /backup/ersatzrs-data-backup.tar.gz -C /restore
```

Recreate the ErsatzRS container with `ersatzrs-data-restored:/data`. Check
**Support** -> **Health Checks** before removing the old container or volume.

## Back up a native installation

Stop ErsatzRS gracefully, then copy the complete state directory. Do not copy
only `ErsatzRS.db` while the server is running: SQLite WAL data may still be in
the adjacent `-wal` file.

For the documented systemd installation:

```bash
sudo tar -czf ersatzrs-state-backup.tar.gz -C /var/lib/ersatzrs .ersatzrs
```

For a native process running as the current user:

```bash
tar -czf ersatzrs-state-backup.tar.gz -C "$HOME" .ersatzrs
```

On Windows, select **Exit** from the tray menu, then copy
`%USERPROFILE%\.ersatzrs` with File Explorer or PowerShell.

## Restore a native installation

1. Stop ErsatzRS.
2. Move the current state directory aside instead of deleting it.
3. Restore the backed-up directory to the same user account's state location.
4. Restore any external database, deployment configuration, definition files,
   and secret references.
5. Start ErsatzRS with the same or a newer application version.
6. Check **Support** -> **Health Checks** and inspect the log for migration or
   checksum errors.

Do not restore a database created by a newer ErsatzRS version into an older
binary. Startup health reporting treats unknown or mismatched migrations as a
schema error.

## Verify the backup

Keep the backup until you have confirmed:

- the admin UI opens;
- health checks show database and schema readiness;
- libraries, channels, schedules, and playouts are present; and
- at least one channel stream starts.
