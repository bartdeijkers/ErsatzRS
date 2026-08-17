# Plan: accept provider-supplied chapters from add-on lists

## Implementation status

Implemented in the working tree on 2026-08-17. Provider extraction and
whole-second rounding remain package-owned; the host accepts only bounded,
provider-neutral timestamp/title text through `remote-stream.item.v2`.
The implementation also removed the former Schatkamer source-grammar adapter
from ErsatzRS; each add-on now owns all provider URL grammar.

## Context

ADR 0070 gives add-on list items authored chapter markers. The host keeps the
full episode item and derives child fragment items "atomically from validated
YouTube-style text": `AddonMediaListItem.ChapterInput` holds the text,
`ersatzrs_core::media_chapters::parse_chapter_input` validates it, and
`append_item_chapter_fragments`
(`crates/ersatzrs-scanner/src/remote_stream_playlist.rs:971`) re-invokes the
add-on with `ERSATZRS_MEDIA_LIST_CHAPTERS` to produce fragments. Each fragment
gets a stable provider ID derived from parent and start time, an
`AddonMediaListChapter` row, and a Remote Stream media item whose
`ParentMediaItemId` points at the full episode.

That input is **operator-authored today**. An administrator opens the per-item
page and types timecodes, even when the provider already publishes them.

yt-dlp exposes chapters when a video has them. Verified 2026-08-17 against
`Leog6MbGABw`:

```json
[{"start_time": 0, "title": "Intro", "end_time": 20},
 {"start_time": 20, "title": "Sculpt Mode", "end_time": ...}]
```

The shape is a direct match for what ADR 0070 already consumes — these *are*
YouTube chapters, the same markers `parse_chapter_input` was written to accept.

Three measured constraints shape the design:

- **Chapters require full extraction.** `--flat-playlist` returns `NA` for
  `chapters`. They are invisible to today's enumeration and only become
  available inside the per-item metadata pass.
- **Chapters are sparse.** 1 of 6 sampled Blender Today livestreams had them;
  0 of 3 Blenderheads episodes and Big Buck Bunny had none. The common case
  must stay free.
- **`start_time`/`end_time` are numbers and may be fractional.** ADR 0070
  specifies whole-second fragment bounds and the wire fields are `Option<u64>`,
  so values need deliberate rounding rather than passthrough.

## Blockers in the current contract

1. **Chapter support is gated on one add-on identity.** Fragment derivation is
   reachable only when the caller is Beeld & Geluid — three separate gates, in
   both surfaces:
   - `crates/ersatzrs-scanner/src/remote_stream_playlist.rs:468` and `:490`
     (`addon_id == BEELDENGELUID_ID`),
   - `crates/ersatzrs-infra/src/addon_media_list.rs:158` and `:178`,
   - and `remote_stream_playlist.rs:529`, which mints fragment GUIDs with a
     hard-coded `beeldengeluid-chapter://` scheme.

   ADR 0070 sanctioned a narrow provider-specific host module for *source
   grammar and normalization*. Host-side fragment identity keyed on a provider
   name is wider than that exception, and it means no other add-on can use
   chapters at all — independently of where the markers come from. **This, not
   enrichment, is the first thing the slice has to resolve.**
2. **yt-dlp declares only `media-list.list.v1`/`v2` and
   `remote-stream.list.v1`/`v2`.** The fragment fields `parent_provider_id`,
   `fragment_start_seconds`, and `fragment_end_seconds`
   (`crates/ersatzrs-addon-contract/src/lib.rs:129-133`) are v3-only. ADR 0070:
   "Earlier versions remain frozen and reject those fields." Beeld & Geluid
   0.5.0 is already on v3.
3. **No wire field carries chapter *text*.** `ChapterInput` is a host-side
   column with no counterpart in either list record, so a provider cannot
   currently offer markers for the host to validate and expand.

Blocker 1 splits the slice usefully. Generalizing chapter derivation off the
provider identity is independent of the metadata enrichment work and would let
any add-on use **operator-authored** chapters immediately, through the existing
per-item editor. Only **provider-supplied** markers depend on enrichment.

## Options

### Option A — the package emits v3 fragment records

yt-dlp moves to `media-list.list.v3` / `remote-stream.list.v3` and emits
chapters as child fragments itself, matching how Beeld & Geluid works.

The package must then also honour ADR 0070's URL contract: fragment URLs carry
`start` and optional `end` query parameters, and both runtimes must strip those
before provider metadata and stream requests, then apply start and duration to
FFmpeg. The full episode URL stays unbounded, and a final chapter omits `end`
only when the duration is unknown.

Largest package change, and it duplicates marker-expansion logic that the host
already owns.

### Option B — the package emits chapter text, the host expands it (recommended)

Add one optional field to the item record carrying YouTube-style chapter text.
The host validates it with the existing `parse_chapter_input` and derives
fragments through the existing `append_item_chapter_fragments`, so marker
expansion, stable fragment identity, and refresh semantics stay in exactly one
place and remain identical for authored and provider-supplied markers.

Smaller package change and no new expansion logic. Two costs to design around:

- The new field is a contract addition, so it needs a new capability version
  under the frozen-wire-contract rule — the same rule that governs the
  per-item enrichment slice.
- `append_item_chapter_fragments` currently performs one **extra** add-on
  invocation per chaptered item. Provider chapters would add that cost on top
  of the metadata pass unless expansion is fed from the metadata response
  instead of a fresh invocation.

Option B is recommended because ADR 0070 already assigns marker validation and
fragment derivation to the host, and Option A would move that responsibility
into every package that wants chapters.

## Work outline

Sequenced after the per-item metadata enrichment slice, which is what makes
`chapters` reachable at all.

1. **Contract** — add the optional chapter-text field to the item record under
   a new capability version, with `deny_unknown_fields` preserved and the
   existing bounded diagnostics reused. Bound the field length.
2. **Host** — accept provider-supplied text as a source for `ChapterInput`, and
   decide precedence against operator-authored text. Recommend: operator text
   always wins, so a manual correction is never overwritten by a later sync;
   record the provider value separately so the UI can offer "restore provider
   chapters".
3. **Fragment derivation** — feed expansion from the enrichment response rather
   than a fresh add-on invocation, so a chaptered item does not cost two calls.
4. **Package** — emit `%(chapters)j` in the per-item response, rounding
   `start_time` to whole seconds, skipping the field entirely when yt-dlp
   returns `NA`, and keeping the POSIX and Windows entrypoints byte-equivalent.
5. **Docs and bookkeeping** — extend `docs/reference/addon-media-lists.md` and
   the Remote Stream playlist manifest reference with the new field and
   capability version; record the precedence rule in an ADR amendment or a new
   ADR; update `docs/progress/parity/remote-stream-playlists.toml`.

## Verification

- A video with chapters produces child fragment items with whole-second bounds
  and a stable per-fragment provider ID; a video without chapters produces
  none and costs no extra provider request.
- Operator-authored chapter text survives a subsequent sync unchanged.
- Fragment playback honours `start`/`end` without those parameters reaching the
  provider request.
- One chaptered item costs one enrichment call, not two.
- Focused scanner and contract regressions, then the workspace closeout gate.

## Related

- ADR 0070 — Schatkamer URL sources and chapter fragments.
- ADR 0068 — provider-neutral host rule; the chapter-text field must stay
  provider-neutral.
- `docs/progress/parity/remote-stream-playlists.toml` — owns yt-dlp list
  behaviour and the open `addon-nfo-v2-closeout` debt.
