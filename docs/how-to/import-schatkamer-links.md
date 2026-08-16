# Import Schatkamer links and chapters

Use the official Beeld & Geluid add-on to turn a public Schatkamer hyperlink
into synchronized Remote Stream media.

## Prerequisites

- Install and enable `org.ersatzrs.addon.beeldengeluid` under **Settings > Add-ons**.
- Make the Schatkamer list or episode publicly accessible without a signed-in browser session.

## Import a link

1. Open **Media Sources > Add-on Lists**.
2. Select **Beeld & Geluid** and paste one of these Schatkamer links:
   a personal `/lijst/<uuid>` link, a `/serie/` or `/programma/` link, one
   `/aflevering/<id>` link, or a `/zoeken` link with filters.
3. Leave **Media kind** on automatic detection unless every result needs the
   same override.
4. Optionally choose a storage-folder name, then select **Add**.

ErsatzRS normalizes the URL, preserves search sorting and filters, traverses
every result page, and queues the managed Remote Streams library scan. A failed
page fails the synchronization; it does not silently replace the last complete
result with a partial list.

## Add chapters to one episode

For an individual episode URL, enter one timecode and title per line in
**Chapters**:

```text
0:00 Introduction
1:35 First topic
4:12 Demonstration
10:08 Common mistakes
15:42 Conclusion
```

Use `M:SS`, `MM:SS`, or `H:MM:SS`. Starts must be unique and increasing and
must fall before the known episode duration. Saving valid input keeps the full
episode and creates one independently selectable fragment per line. Clearing
the field removes the fragments on synchronization while keeping the full
episode.

If validation fails, correct the reported line before saving. ErsatzRS does
not apply a partially valid chapter set.

See the [add-on media-list contract](../reference/addon-media-lists.md) for the
stored identity and fragment fields.
