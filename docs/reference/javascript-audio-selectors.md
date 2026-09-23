# JavaScript audio selectors

Native playback and the optional Next engine can select movie and episode audio
with JavaScript. Place `movie.js` and/or `episode.js` under
`<application-data>/scripts/audio-stream-selector/`. `ERSATZRS_APP_DATA_DIR`,
when configured, selects that application-data root. Files are read for each
selection; no restart is needed to change a selector. Each selection uses a fresh
JavaScript runtime; derive decisions from the supplied arguments rather than
relying on globals from a previous selection.

Exact stream pins, including explicit silence, take precedence. A successful
custom YAML audio selection also takes precedence. Otherwise JavaScript runs
before the normal language/title selection. Missing scripts, null or invalid
results, exceptions and resource-limit failures use that normal fallback.
Subtitle selection is independent. Raw HLS Direct without an effective language
or title preference keeps all source audio tracks and bypasses JavaScript; an
explicit original-language (`und`) choice remains a selection preference.

## Callbacks

```javascript
function selectMovieAudioStreamIndex(
    channelNumber, channelName, title, guids, preferredLanguageCodes, audioStreams
) {
    return audioStreams.find(stream => stream.language === "nld")?.index ?? null;
}

function selectEpisodeAudioStreamIndex(
    channelNumber, channelName, showTitle, showGuids, seasonNumber,
    episodeNumber, episodeGuids, preferredLanguageCodes, audioStreams
) {
    return null;
}
```

Use the movie callback in `movie.js` and the episode callback in `episode.js`.
Channel number and name are strings. Titles can be null. GUID arrays use
`tmdb_123` rather than `tmdb://123`. Preferred language codes include supported
two- and three-letter forms. Each audio object exposes `index`, `channels`,
`codec`, `isDefault`, `isForced`, `language` and `title`; nullable source fields
remain null. A numeric return value is truncated as in upstream and must identify
an existing audio stream. Strings, booleans and nonexistent indexes are rejected.

## Limits and diagnostics

The embedded QuickJS runtime has a 4,000,000-byte heap limit, a 256 KiB stack
limit and a four-second execution/admission deadline. At most two blocking
workers execute selectors concurrently. Cancelling a selection interrupts its
worker; capacity is retained until that worker exits. Each script and serialized
metadata request is limited to 256 KiB, with at most 256 audio streams or GUIDs
per collection. Oversized inputs fall back rather than silently truncate.

No CLR/.NET, filesystem, network, module-loader or process APIs are exposed.
Blocking `Atomics.wait` is unavailable.
`log(...)` is accepted but discards its arguments to avoid exposing private
metadata or producing unbounded logs. Loading and execution failures generate
fixed, redacted diagnostics. The **JavaScript Audio Selection** health check
reports failures recorded since startup; its counter resets on restart. A clean
counter does not validate scripts that have not executed.

These limits deliberately differ from Jint's exact statement counter and CLR
interop; see [ADR 0096](../adr/0096-bounded-javascript-audio-selectors.md).
Platform acceptance is tracked by UCU-02 and the upstream catch-up milestones.
