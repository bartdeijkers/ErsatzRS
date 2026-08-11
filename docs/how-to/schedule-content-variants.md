# Schedule extras and alternate versions

Use this guide to include content that ordinary searches intentionally hide,
such as featurettes, alternate renditions, or movies with commentary tracks.
For the general save-and-schedule workflow, first read [Keep channel content
updated with a smart collection](keep-channel-content-updated-with-a-smart-collection.md).

## Before you begin

Scan the library and open the relevant media detail page. Confirm that ErsatzRS
classified the item as an extra or alternate version, or detected the intended
commentary audio stream. A smart collection cannot select metadata that the
scan did not persist.

## Schedule unplugged renditions

Create a smart collection with:

```text
Name: Example - Unplugged Renditions
Query: content_kind:alternate_version AND sub_kind:unplugged
```

Attach the smart collection to a schedule item. The explicit `content_kind`
and `sub_kind` terms select unplugged alternate versions without adding primary
music videos or songs.

## Schedule commentary movies

Create a smart collection with:

```text
Name: Example - Commentary Movies
Query: type:movie AND has_commentary:true AND audio_title:commentary
```

Attach the smart collection to a movie schedule. Set the channel's preferred
audio title, or the custom stream selector's audio allowlist, to `commentary`.
This selects both a movie that has a commentary stream and the intended stream
at playback time.

## Add a featurette before each movie

Create a smart collection for featurettes:

```text
Name: Example - Featurette Pre-Roll
Query: content_kind:extra AND sub_kind:featurette
```

Create a filler preset:

```text
Name: Example - Featurette Pre-Roll Filler
Filler kind: PreRoll
Filler mode: Count
Count: 1
Collection type: SmartCollection
Smart collection: Example - Featurette Pre-Roll
Allow watermarks: false
```

Attach the preset to the **PreRoll** field of the movie schedule item. Keep the
main movie source primary-only with a normal collection or this smart
collection:

```text
Name: Example - Movie Primaries
Query: type:movie
```

Ordinary searches implicitly keep `content_kind:primary`, so the main
collection remains primary-only and the featurette enters through the explicit
pre-roll filler.

## Verify the schedule

Open each smart collection and confirm that it contains the intended variants
without their primary items. Save the affected schedule, wait for its Classic
playout refresh, and inspect **Playout Detail**. For commentary, also start one
scheduled item and confirm that the selected audio stream has the expected
title.

If a collection is empty, return to the media detail page and compare the
persisted content kind, sub-kind, and stream title with the query. Rescan after
correcting filenames, sidecars, or stream metadata.
