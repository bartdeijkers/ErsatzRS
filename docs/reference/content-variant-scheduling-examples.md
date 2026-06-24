# Content Variant Scheduling Examples

ADR 0018 content variants stay hidden from ordinary searches and schedules by
default. Use these examples when an operator intentionally wants special
features, alternate versions, or commentary-bearing items in a channel.

## Unplugged Renditions

Create a smart collection:

```text
Name: Example - Unplugged Renditions
Query: content_kind:alternate_version AND sub_kind:unplugged
```

The explicit `content_kind` / `sub_kind` terms opt into alternate-version rows,
so the collection selects unplugged renditions without selecting canonical
primary music videos or songs.

## Commentary Movies

Create a smart collection:

```text
Name: Example - Commentary Movies
Query: type:movie AND has_commentary:true AND audio_title:commentary
```

Attach this smart collection to a movie channel and set the channel preferred
audio title, or the custom stream-selector audio allowlist, to `commentary`.
That combines the search-side stream-role selection with the playback-side
stream selector.

## Featurette Pre-Roll

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

Attach that filler preset to the `PreRoll` field of the movie schedule item.
The main movie item can still use a normal primary-only movie collection, for
example:

```text
Name: Example - Movie Primaries
Query: type:movie
```

Because ordinary searches implicitly keep `content_kind:primary`, this movie
collection stays primary-only and the featurette appears only through the
explicit pre-roll filler.
