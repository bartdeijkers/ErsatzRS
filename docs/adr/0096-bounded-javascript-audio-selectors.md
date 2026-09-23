# ADR 0096: Embed bounded JavaScript audio selectors

## Status

Proposed

## Implementation

Not implemented

## Context

UCU-02 row 21 (`e6496bbc`, upstream PR #2866) connects Next playout audio
selection to `FFmpegStreamSelector`. That selector first attempts the existing
movie/episode JavaScript hook, then falls back to language/title selection.
ErsatzRS currently has the language/title and custom YAML paths, but no runtime
for `scripts/audio-stream-selector/movie.js` or `episode.js`. Therefore the row
cannot be classified as Covered merely because ordinary alternate audio works.

Upstream Jint exposes `selectMovieAudioStreamIndex` and
`selectEpisodeAudioStreamIndex`, supplies metadata and audio-stream objects,
and limits execution to four seconds, 4,000,000 bytes and 1,000 statements.
Its wrapper also enables CLR access. Existing Lua scheduling and YAML selectors
do not execute this JavaScript contract. No existing ADR accepts its omission.

## Decision

Propose an embedded QuickJS interpreter through `rquickjs`, contained within
`ersatzrs-infra` behind a provider-independent audio-selection adapter. Evaluate
the documented 0.14 API first; pin a compatible version in Cargo.lock only after
license, MSRV and platform verification. No Node executable, external service,
new workspace crate or graphics renderer is introduced.

Preserve the existing script paths, function names, positional arguments,
metadata GUID normalization (`://` to `_`) and audio fields `index`, `channels`,
`codec`, `isDefault`, `isForced`, `language`, `title`. Supply channel identity,
movie title/GUIDs or show/episode identity, and both two- and three-letter
preferred language codes. Keep custom YAML selection ahead of this fallback;
explicit media/stream pins remain authoritative under ADR 0056. Missing scripts,
null/invalid results and execution failures fall back to ordinary audio selection.
Only an existing audio index may be selected; preserve explicit silence.

Use the same adapter for native playback and Next projection, retaining their
existing seek, subtitle and HTTP-source behavior. Script execution receives
owned metadata, never database connections or provider credentials. Bound
blocking worker admission, keep the permit with the worker, and interrupt on
cancellation or a four-second deadline. Configure a 4,000,000-byte engine heap
limit with the engine allocator, plus a bounded stack and script-file size.
Do not use `rust-alloc`/custom allocation modes that disable the documented heap
limit. Test that timeout/cancellation actually returns execution capacity.

Explicit portability adaptations requiring this decision:

- Do not expose Jint CLR/.NET interop, filesystem, network, module loading or
  process APIs to scripts. Preserve the documented selector callbacks and a
  controlled diagnostic `log` function; explain unsupported CLR usage.
- Use engine memory/stack limits and interrupt deadlines rather than claim an
  exact mapping of Jint's 1,000-statement counter to QuickJS execution steps.
  No unbounded thread is an acceptable timeout implementation.

## Consequences

The embedded engine adds a C build dependency and transitive Rust dependencies.
`rquickjs` documents memory limits and interrupt callbacks, but its platform
matrix does not establish all six ErsatzRS targets: ARMv7 needs explicit proof,
and Windows MSVC is described as experimental. Verify these two build risks
before treating the dependency choice as viable. The existing program/platform
acceptance retains all six native targets; record which builds and native
fixture runs are proven in the owning milestone. Missing platform evidence
cannot be relabeled success or silently disable the feature on that target.
This proposal does not move hosted CI ahead of the authorized UCU-03 boundary.

Before row acceptance, prove movie and episode argument shapes, GUID mapping,
custom/pin precedence, missing/null/throw/invalid-index fallback, bounded loops,
allocation failure, cancellation, and unchanged source audio/subtitle selection.
Decode a Next stream where JavaScript chooses the non-default audio track.
Update runtime health/documentation for selector loading/execution failures
without logging script contents, private metadata or credentials. Keep UCU-02
and the row pending until their required evidence exists. No new dependency or
runtime implementation is authorized by the Proposed status alone.

## Alternatives Considered

- Existing Lua or YAML only: cannot execute the upstream JavaScript callbacks.
- Handwritten JavaScript subset: would silently reject legitimate selectors.
- Boa: pure Rust is attractive, but the inspected runtime-limit API does not
  establish a hard heap bound equivalent to the upstream limit; not selected
  without that evidence.
- External Node/.NET runtime: adds a runtime installation/process boundary to
  a small synchronous selection hook; not selected.

## References

- [ADR 0001](0001-record-architecture-decisions.md)
- [ADR 0056](0056-exact-media-version-and-stream-selection.md)
- [Catch-up procedure](../../plans/upstream-catch-up.md), step 4
- `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegStreamSelector.cs`
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Scripting/ScriptEngine.cs`
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/Repositories/StreamSelectorRepository.cs`
- [rquickjs Runtime API](https://docs.rs/rquickjs/0.14.0/rquickjs/struct.Runtime.html)
- [rquickjs platform matrix and MIT license](https://github.com/DelSkayn/rquickjs)
- [Boa runtime limits](https://docs.rs/boa_engine/0.21.1/boa_engine/vm/struct.RuntimeLimits.html)
