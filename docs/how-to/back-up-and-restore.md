# Back up and restore ErsatzRS

Use this guide before upgrades, host migrations, or database experiments.

## Identify the database and state paths

Before stopping ErsatzRS, determine the active database from
`ErsatzRS_DATABASE__URL`, `ERSATZRS_DATABASE__URL`, or `DATABASE_URL`. When none
is set, the default SQLite database lives in the application state directory.
See the [configuration reference](../reference/configuration.md#startup-database-url)
for precedence and defaults.

Back up a custom SQLite database at the path in its URL. For MySQL or MariaDB,
follow [Preserve a historical MySQL deployment](#preserve-a-historical-mysql-deployment)
before moving to a SQLite-only release; copying an application directory does
not back up that database.

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
4. Restore deployment configuration, definition files, and secret references.
5. Start ErsatzRS with the same or a newer application version.
6. Check **Support** -> **Health Checks** and inspect the log for migration or
   checksum errors.

Do not restore a database created by a newer ErsatzRS version into an older
binary. Startup health reporting treats unknown or mismatched migrations as a
schema error.

## Preserve a historical MySQL deployment

ADR 0080 retires the source-built MySQL/MariaDB variant. There is not yet a
verified MySQL-to-SQLite data converter or import procedure. A SQL dump from
`mysqldump` or `mariadb-dump` cannot be loaded directly into SQLite: SQL syntax,
generated identities, binary values, `NULL` encoding, and foreign-key handling
differ. Do not use an online converter against the only copy of a database, and
do not start a SQLite-only ErsatzRS release against a MySQL URL.

Before changing the old installation, preserve enough evidence to reproduce
and validate a later offline conversion:

1. Record the exact Git revision or release tag of the running MySQL-capable
   ErsatzRS build. Keep its binary or source checkout and configuration. The
   release migration guide must name the final MySQL-capable revision; do not
   assume an older tag understands a newer migration history.
2. Stop the web host and scanner so no ErsatzRS process can write during the
   export.
3. Use a client option file readable only by the operator instead of putting
   credentials on the command line. Verify the source before dumping:

   ```bash
   mysqlcheck --defaults-extra-file=./mysql-client.cnf --check ersatzrs
   ```

4. Create a transactionally consistent, schema-and-data backup with the native
   tool matching the server family:

   ```bash
   mysqldump --defaults-extra-file=./mysql-client.cnf \
     --single-transaction --quick --routines --triggers --hex-blob \
     --databases ersatzrs > ersatzrs-mysql.sql
   sha256sum ersatzrs-mysql.sql > ersatzrs-mysql.sql.sha256
   ```

   Use `mariadb-dump` with the same options for MariaDB. Keep the dump, hash,
   exact ErsatzRS revision, application state archive, and deployment
   configuration together. Store the client option file separately because it
   contains credentials.
5. Restore the dump into a separate scratch MySQL/MariaDB database and run the
   recorded MySQL-capable ErsatzRS revision against that copy. Confirm health,
   libraries, channels, schedules, playouts, and one stream before treating the
   backup as usable. Never test the restore over the source database.

This is a deterministic preservation procedure, not a SQLite conversion. The
first SQLite-only release remains blocked for existing MySQL operators until a
release-specific offline runbook pins the final MySQL-capable revision and
proves the complete table transfer, migration-history mapping, identity and
foreign-key preservation, `NULL`/text/blob handling, SQLite integrity checks,
ErsatzRS health checks, and rollback from the preserved dump. Until then,
continue running the recorded MySQL-capable revision or initialize a new SQLite
installation and reconfigure it manually.

## Verify the backup

Keep the backup until you have confirmed:

- the admin UI opens;
- health checks show database and schema readiness;
- libraries, channels, schedules, and playouts are present; and
- at least one channel stream starts.
