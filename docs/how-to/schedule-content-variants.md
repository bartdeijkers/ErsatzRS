# Schedule extras and alternate versions

Use this guide to include content that ordinary searches intentionally hide,
such as featurettes, alternate renditions, or movies with commentary tracks.

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
