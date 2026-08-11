# Build a channel from a collection

Use a Classic schedule to turn scanned media into a populated, playable
channel. You will create or select the collection and channel, build their
playout, and verify the generated timeline.

## Before you begin

You need:

- a scanned library with playable media;
- a manual collection, or media ready to add to one;
- an enabled channel, or a channel number and name to create; and
- an FFmpeg profile that is ready according to **Support** -> **Health Checks**.

If the media or collection does not exist yet, first [add and scan a local
library](add-and-scan-a-local-library.md). The [admin navigation
reference](../reference/admin-navigation.md) lists the stable routes used below.

## Prepare the collection and channel

If the content is not in a reusable collection yet:

1. Follow [Build a manual collection from media
   results](build-a-manual-collection.md).
2. Return here after confirming its member list.

If there is no suitable channel:

1. Open **Channels** and select **Add Channel**.
2. Enter a unique channel number and a recognizable name.
3. Select a ready FFmpeg profile and save the channel.

Keep the collection small for the first build so the generated order is easy
to verify.

## Create the schedule

1. Open **Scheduling** -> **Schedules**.
2. Select **Add Schedule**.
3. Give the schedule a name and save it.
4. Select **Add Schedule Item**.
5. Choose **Collection** as the collection type.
6. Select the collection that should supply the channel content.
7. Choose a playback order.
8. Choose **Flood** as the playout mode for a continuously filled first
   schedule.
9. Select **Save Schedule Items**.

## Rotate several collections

Do not use unbounded **Flood** items when the schedule should advance through
several dynamic rows: Flood continues until a later fixed start bounds it.

1. Add one schedule item for each collection.
2. Choose **One** to play one item before advancing, or **Multiple** and a count
   to play a short run from each source.
3. Use **Move Up** and **Move Down** to set the rotation order.
4. Select **Save Schedule Items**.
5. After the refresh, inspect **Playout Detail** and confirm that the collection
   sources alternate in the intended order.

## Attach the schedule to a channel

1. Open **Scheduling** -> **Playouts**.
2. Select **Add Playout** -> **Classic Schedule**.
3. Select the target channel.
4. Select the schedule as the program schedule.
5. Select **Add Playout**.

A channel that already has a playout is unavailable in the add dialog. Edit or
remove the existing playout instead of creating a second timeline for the same
channel.

## Verify the generated timeline

1. Return to **Scheduling** -> **Playouts**.
2. Open **Playout Detail** for the new row.
3. Confirm that scheduled items have start and finish times and come from the
   intended collection.
4. Open the channel stream in a player:

   ```text
   http://<ersatzrs-host>:8409/iptv/channel/<channel-number>.m3u8
   ```

If the detail page is empty, confirm that the collection contains available
media and that the schedule item was saved. Then use **Refresh Playout** and
check **Support** -> **Health Checks**.

## Apply later content or schedule changes

Saving an impacting collection or Classic schedule change queues a playout
refresh automatically. Reopen **Playout Detail** after the queued build
finishes and confirm that the future timeline reflects the change.

Use the row's **Refresh Playout** action when you need to request that rebuild
explicitly, for example after repairing unavailable media. Refresh preserves
the current anchor while rebuilding the affected window; it is not a request
to restart the channel at the first collection item.

To import the completed channel into another application, follow [Connect an
IPTV client](connect-an-iptv-client.md). The [HTTP endpoint
reference](../reference/http-endpoints.md) owns the exact playlist, guide, HLS,
and MPEG-TS paths.
