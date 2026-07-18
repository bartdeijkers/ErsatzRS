# Add and scan a local library

Use this guide to import media from directories that are visible to the
ErsatzRS process.

## Make the files visible

The server must be able to read every configured path.

- A native installation uses host paths.
- A Docker installation uses container paths. Mount host media into the
  container first, for example `/path/to/movies:/media/movies:ro`, then enter
  `/media/movies` in ErsatzRS.
- Network shares should be mounted by the host before starting ErsatzRS. Do not
  put credentials in a library path or in documentation screenshots.

One local library has one media kind, but it can contain multiple paths.

## Add the library

1. Open the admin UI.
2. Expand **Media Sources** and select **Local**.
3. Select **Add Library**.
4. Enter a name.
5. Select one media kind: **Movies**, **TV Shows**, **Music Videos**,
   **Other Videos**, **Songs**, **Images**, or **Remote Streams**.
6. Enter one path per line under **Library Paths**.
7. Select **Add**.

The media kind cannot be changed after the library is created. Create another
library when the same root contains a different kind of content.

## Run a scan

1. Open **Media** -> **Libraries**.
2. Find the new library.
3. Select **Scan Library**.
4. Wait for the queued scan and progress indicator to finish.
5. Use the row's **Search Media** action or open the matching **Media** page to
   inspect imported items.

Scans are serialized per library. If a scan is already running, wait for its
indicator to finish before starting another.

## Rescan after changes

Use a normal scan after adding, renaming, or removing files. Provider libraries
also expose a **Deep Scan Library** action; local libraries use the normal scan
path.

When items show **File Not Found** or **Unavailable**, check that the configured
path still resolves from the server's own filesystem view. For Docker, verify
the mount directly:

```bash
docker exec ersatzrs find /media -maxdepth 3 -type f
```

## Verify tracks and variants

Open a movie, episode, or other media detail page to inspect metadata, audio
tracks, subtitles, extras, and linked alternate versions. ErsatzRS uses this
information for channel-level and schedule-item playback preferences.

If a file imports without the expected streams, open **Support** ->
**Troubleshooting** for that media item and inspect the ffprobe result and
resolved path.
