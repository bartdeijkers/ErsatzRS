# Keep channel content updated with a smart collection

Use a smart collection when channel eligibility should follow media metadata
instead of a manually maintained member list. New or rescanned items can then
match the saved search without being added one by one.

## Before you begin

Scan the relevant libraries and confirm that the metadata you intend to search
is visible on the media detail pages. Start with a narrow query whose complete
result set you can inspect.

## Build and verify the search

1. Open **Search media** in the top bar.
2. Enter a query, for example:

   ```text
   genre:Adventure AND type:movie
   ```

3. Inspect every result page and refine the query until it includes only media
   that may play on the channel.
4. Select **Save As** to open **Save As Smart Collection**.
5. Choose **New Collection**, enter a recognizable name, and select **Save**.
6. Confirm the **Smart collection saved** notification.

Do not save a broad query merely because its first result page looks correct.
The schedule uses every matching available item.

## Attach it to a Classic schedule

1. Open **Scheduling** -> **Schedules** and edit the target schedule.
2. Add a schedule item or edit an existing one.
3. Choose **Smart Collection** as the collection type.
4. Select the saved smart collection, choose the intended playback order and
   playout mode, and save the schedule items.
5. Wait for the Classic playout refresh, then inspect **Playout Detail**.

The future timeline should contain only items that matched the saved search.
Use **Refresh Playout** after a later rescan when you need the changed result
set to appear in the materialized schedule immediately.

## Correct an unexpected result

Open **Lists** -> **Smart Collections** and use **Search Smart Collection** to
recheck the current matches. If an item is missing or unexpected, inspect its
persisted metadata before changing the query. Edit and save the query, refresh
the affected playout, and verify the future timeline again.

For exact page locations, see the [admin navigation
reference](../reference/admin-navigation.md). For recipes that target extras,
alternate versions, or commentary streams, use [Schedule extras and alternate
versions](schedule-content-variants.md).
