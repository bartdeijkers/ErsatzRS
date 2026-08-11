# Connect a Jellyfin server

Use this guide to import selected Jellyfin libraries into ErsatzRS.

Synchronization is one-way: Jellyfin remains the media and metadata source,
and ErsatzRS does not modify the Jellyfin server or its libraries.

## Prepare Jellyfin

1. Open the Jellyfin administration dashboard and its **API Keys** page.
2. Add a key named `ErsatzRS` and copy its value for the connection form.
3. Confirm that ErsatzRS can reach the Jellyfin server address.
4. If ErsatzRS runs in Docker, use an address reachable from the container,
   such as `http://jellyfin:8096`; do not use `localhost` unless both services
   share the same network namespace.

Treat the API key as a secret. Do not include it in screenshots or support
logs.

## Configure the connection

1. In ErsatzRS, open **Media Sources** -> **Jellyfin**.
2. Select **Configure Connection**.
3. Enter the Jellyfin server address, including `http://` or `https://` and any
   non-default port.
4. Enter the API key.
5. Select **Save**.

ErsatzRS validates the key against Jellyfin before saving. After a successful
save, the Jellyfin page shows the detected server name and address.

## Select libraries to synchronize

1. On the Jellyfin source row, select **Libraries**.
2. Enable **Sync** for each library that should be available to ErsatzRS.
3. Select **Save**.

Saving starts synchronization for the selected libraries. Open **Media** ->
**Libraries** to follow progress, then open **Movies** or **TV Shows** to
inspect imported items. Jellyfin discovery currently accepts those two library
kinds.

## Map provider paths when needed

Add a path replacement when Jellyfin reports a file path that is different
from the path visible to ErsatzRS. This is common when the two applications use
different container mounts or operating systems.

No replacement is needed when both applications already see the same path. A
replacement only rewrites a path prefix; it does not copy or move media.

1. On the Jellyfin source row, select **Paths**.
2. Enter the path prefix reported by Jellyfin under **Jellyfin Path**.
3. Enter the equivalent path visible to ErsatzRS under **Local Path**.
4. Add the replacement and select **Save**.
5. Rescan the affected library from **Media** -> **Libraries**.

For example, map a Jellyfin path such as `/library/movies` to the ErsatzRS
container path `/media/movies`. Use directory prefixes, not one mapping per
file.

## Verify the result

Open an imported media item's detail page and confirm that its file is
available and its audio, subtitle, and video streams are present. If the item
is unavailable, use **Support** -> **Troubleshooting** to compare the provider
path with the resolved ErsatzRS path.

See [Media source support](../reference/media-source-support.md) for the current
Plex and Emby connection status.
