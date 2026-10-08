# Build your first channel

This tutorial takes you from a new Windows installation to a playable channel.
You will start the Windows release archive, scan a small local movie library,
create a collection and schedule, and open the resulting stream.

You will finish with a working example of the main ErsatzRS path:

```text
media source -> library scan -> collection -> schedule -> playout -> channel
```

This is a learning path rather than an exhaustive operator reference.

## Before you begin

You need:

- Windows x86-64;
- the current `ersatzrs-<tag>-win-x64.zip` release archive;
- a directory containing at least one movie that FFmpeg can read;
- port `8409` available on the host; and
- internet access during the first startup so ErsatzRS can acquire its managed
  FFmpeg 9 runtime.

## 1. Start ErsatzRS

1. Extract the complete release folder from the zip to a stable location.
2. Run `ErsatzRS.exe` from that folder.
3. Keep the tray icon running. The launcher starts the server in the
   background.

If Microsoft Defender SmartScreen appears, first confirm that the archive came
from the official ErsatzRS release page. Do not bypass the warning for an
archive from another source.

The first start can take longer because the server downloads and validates the
managed FFmpeg runtime. Check readiness from PowerShell:

```powershell
Invoke-WebRequest -Uri http://127.0.0.1:8409/healthz -UseBasicParsing
```

The command should return HTTP status `200`. Use the tray menu's **Open Web
UI** action, or open <http://127.0.0.1:8409>, after it succeeds. If it does not
succeed, select **Show Logs** from the tray menu and wait for the managed
FFmpeg setup to finish.

## 2. Add a local movie library

In the navigation drawer:

1. Expand **Media Sources** and select **Local**.
2. Select **Add Library**.
3. Enter `Tutorial Movies` as the name.
4. Select **Movies** as the media kind.
5. Enter the full path to your movie directory in **Library Paths**.
6. Select **Add**.

Enter one directory per line when the library spans several folders.

## 3. Scan the library

1. Open **Media** -> **Libraries**.
2. Find `Tutorial Movies`.
3. Select its **Scan Library** refresh action.
4. Wait for the scan indicator to finish.
5. Open **Media** -> **Movies** and confirm that your files appear.

If the scan finishes without media, confirm the directory still exists and is
readable by the Windows account that started ErsatzRS.

## 4. Create a collection

1. Open **Lists** -> **Collections**.
2. Select **Add Collection**.
3. Name it `Tutorial Movies` and save it.
4. Open the new collection.
5. Use the member search to add two or more scanned movies.

A collection is a reusable content source. A schedule decides how and when to
draw items from that source.

## 5. Create a schedule

1. Expand **Scheduling** and open **Schedules**.
2. Select **Add Schedule**.
3. Name it `Tutorial Schedule`.
4. Save the schedule. ErsatzRS opens its schedule items.
5. Select **Add Schedule Item**.
6. Choose **Collection** as the collection type.
7. Select `Tutorial Movies`.
8. Choose **Shuffle** as the playback order.
9. Leave the item's **Smart Shuffle** section set to **Off**.
10. Choose **Flood** as the playout mode.
11. Save the schedule items.

`Flood` keeps selecting content from the collection to fill the available
timeline. Later, you can combine fixed start times, duration limits, filler,
watermarks, stream overrides, and Smart Shuffle rules.

## 6. Attach the schedule to a channel

Fresh installations include channel `1 - ErsatzRS` and default FFmpeg profiles.

1. Open **Scheduling** -> **Playouts**.
2. Select **Add Playout** -> **Classic Schedule**.
3. Select channel `1 - ErsatzRS`.
4. Select `Tutorial Schedule` as the program schedule.
5. Select **Add Playout**.

Return to **Playouts**, select the row, and confirm that **Playout Detail** lists
scheduled media. A newly created or refreshed playout can take a moment to fill.

## 7. Play the channel

Open the HLS stream in VLC or another compatible player:

```text
http://127.0.0.1:8409/iptv/channel/1.m3u8
```

The MPEG-TS form is:

```text
http://127.0.0.1:8409/iptv/channel/1.ts
```

For an IPTV client, use the complete playlist and guide URLs:

```text
http://127.0.0.1:8409/iptv/channels.m3u
http://127.0.0.1:8409/iptv/xmltv.xml
```

Replace `127.0.0.1` with the ErsatzRS host name or LAN address when the player
runs on another device.

## 8. Check system health

Open **Support** -> **Health Checks**. Resolve error-level results before relying
on the channel. Use **Support** -> **Logging** for runtime messages and
**Support** -> **Troubleshooting** for media, schedule, playout, and channel
diagnostics.

You now have the complete minimum channel pipeline. Return to the
[documentation home](../README.md) to choose the next task, or use the
[reference index](../reference/README.md) when you need exact contracts.
