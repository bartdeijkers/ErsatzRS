# Human-readable filenames for add-on list items (`{title}-{id}.{ext}`)

## Context

The yt-dlp add-on list sync writes one Remote Stream definition per entry into
the managed folder, plus files derived from it: `.nfo`, artwork
(`{stem}.jpg`, `{stem}-poster.*`, `{stem}-fanart.*`), and subtitles
(`{stem}.{lang}.srt`). Today the stem is only the provider id
(`managed_file_name`, `crates/ersatzrs-scanner/src/remote_stream_playlist.rs:3091`),
which makes it hard to find an episode by name on disk.

Goal: new names look like `{title}-{id}.yml` (and every file derived from it
follows). The title is untrusted provider or operator data, so it must be
cleaned so it is a safe single filename on Windows, macOS and Linux and cannot
be used to inject a path.

Maintainer decision: **the name is fixed once**. It is chosen when the entry is
first written (or once during migration of an existing `{id}.yml`) and stays
the same when the title changes later.

Why renaming is safe for the database: `upsert_local_remote_stream`
(`crates/ersatzrs-scanner/src/lib.rs:1857`) first matches existing items on
`(AddonId, ProviderId)` and then updates `MediaFile.Path`/`PathHash`. So the
MediaItem, playout and history identity survives a path change. The existing
test "reidentified generated path" (`lib.rs:~7180`) covers this area. Sidecar
readers (health check, `addon_publication_fs.rs`, web host) take file names
from `.ersatzrs-playlist.json`. They never rebuild them from the id, so the
sidecar format does not change and there is no version bump.

## Implementation

### 1. Portable title stem (ersatzrs-infra)
`crates/ersatzrs-infra/src/addon_media_storage.rs` already has
`safe_folder_name` + `is_windows_reserved_name` (allowlist: Unicode
`is_alphanumeric`, space, `-`, `_`. Other runs become one `-`. Trim ` .-`).
- Move its character loop into a private shared core. `safe_folder_name` must
  produce exactly the same output as today (the existing test
  `folder_names_are_portable_and_displayable` guards this).
- Add `pub fn portable_title_stem(title: &str, max_bytes: usize) -> Option<String>`:
  same allowlist, whitespace collapsed to one space, trim ` .-` at both ends
  (Windows trailing dot/space), truncate at a UTF-8 char boundary within
  `max_bytes` and re-trim. Return `None` when nothing is left.
- What the allowlist guarantees: no `/ \ : * ? " < > |`, no `.` (so no `..`,
  hidden files or extension tricks), no control or ANSI or newline characters,
  and no bidi or zero-width format characters (Unicode `Cf` is not
  alphanumeric). International titles (é, 日本語, …) are kept.

### 2. Naming in the scanner (`remote_stream_playlist.rs`)
- Split the existing id logic into `managed_id_part(id)`: the id if
  ASCII-alnum/`-_` and ≤120, else `hex_hash`. `legacy_managed_file_name(id)` =
  `{id_part}.yml` (today's behavior, still used for migration detection).
- `titled_managed_file_name(title, id)`: stem budget 200 bytes →
  `portable_title_stem(title, 200 - 1 - id_part.len())`. Result is
  `{title}-{id_part}.yml`, or `{id_part}.yml` when there is no title left.
  Apply `_` prefix via `is_windows_reserved_name` to the final stem for
  completeness. The longest derived name (`-fanart.webp`, `.{lang}.srt`) then
  stays well under the 255-byte component limit.
- `validated_managed_definition_name(name)`: single `Component::Normal`, ends in
  `.yml`, does not start with `.`, no `\`/`:`/control chars, ≤ 230 bytes. Same
  spirit as `validated_subtitle_sidecar_path` (`:1834`).
- Choose the name per entry in the loop at `:1083` (replaces
  `managed_file_name(&entry.id)`). `entry.title` is already the effective title
  after `apply_reviewed_metadata` (`:1053`):
  1. **Existing, non-legacy record** (`previous_entry.file` ≠ legacy name and
     passes validation) → reuse unchanged (fixed name).
  2. **Legacy record** (`previous_entry.file` == `{id_part}.yml`) → target =
     `titled_managed_file_name(previous_entry.title, id)`. Use the title from
     the previous sidecar, not the new one. That sidecar only changes after
     publication, so a retry after a crash or a failed sync picks the same
     target name. Then migrate (step 3).
  3. **No record** → first adopt an orphan: a `*-{id_part}.yml` in the folder
     (single dir listing per sync, parsed only for entries without a record)
     whose name passes validation, that is not owned by another sidecar
     entry, and whose YAML `provider_id` matches. This covers a failed sync
     between writing and sidecar publication in which the title changed in
     the meantime. Otherwise use `titled_managed_file_name(entry.title, id)`.
  4. A sidecar name that fails validation is never reused. Fall back to a
     freshly computed name (this is the defence against a hand-edited sidecar).

### 3. One-time migration of the file family
New helper `migrate_entry_files(folder, previous: &PlaylistSidecarEntry, new_file) -> Result<PlaylistSidecarEntry>`,
called before generation when the old and new stems differ:
- Rename within the same folder (`std::fs::rename`): `.yml`, `.nfo`
  (`nfo_file`), tracked artwork (`artwork`, `artwork_roles`, operator-owned
  included), untracked operator artwork with the old stem
  (`{old}.{jpg,jpeg,png,webp}`, `{old}-poster.*`, `{old}-fanart.*`, same set
  as `existing_operator_artwork` / `artwork_file_name`), and subtitles from
  `previous.subtitles` (via `validated_subtitle_sidecar_path`).
- Idempotent: old path missing and new path present means already done.
  Destination exists **and** source exists means error (never overwrite),
  which becomes a normal failed sync.
- Return a copy of `previous_entry` with rewritten `file`/`nfo_file`/artwork
  and subtitle `file` fields. The rest of the loop (NFO reconcile, which keeps
  operator edits, artwork hash/ownership checks, `materialize_selected_subtitles`)
  then works on the new names without further changes.
- The "Keep" branch for removed entries (`:1287`) keeps its old names (no migration).

### 4. Docs and records
- `CHANGELOG.md` `[Unreleased]` → **Changed**: managed add-on list items now
  use `{title}-{id}` filenames. Existing `{id}` files are renamed once on the
  next sync. Names stay stable after that.
- `docs/progress/parity/remote-stream-playlists.toml`: record the naming
  contract in the summary, then `cargo xtask progress check`.
- Save this plan to `plans/addon-item-title-filenames.md` (repo convention).

## Tests (scanner unit tests in `remote_stream_playlist.rs`, infra unit tests)
- `portable_title_stem` table: `"Star Trek: TNG / Best?"`, `"../../etc/passwd"`,
  `"C:\\evil"`, `"a\u{202E}b"`, `"line\nbreak\x1b[31m"`, `"CON"`, `"日本語のタイトル"`,
  `"Één — aflevering"`, `"  ...  "` → `None`, long multibyte title truncated at
  a char boundary within the budget. Existing folder-name test unchanged.
- New entry → `.yml`/`.nfo`/artwork/`.srt` all share `Title-id` stem.
- Stable name: second sync with a changed title keeps the filename.
- Legacy migration: sidecar with `one.yml` + NFO containing an operator edit +
  artwork + subtitle → all renamed, contents/edit preserved, no files left at
  the old stem, sidecar updated. Partial rename (only `.nfo` moved) finishes
  correctly.
- Sidecar with `file: "../x.yml"` is not reused.
- Orphan adoption after a failed sync with a changed title (no duplicate `.yml`).
- Adjust existing tests that expect `one.yml`/`two.yml` (e.g. `:5921`, `:6344`–`:6406`).

## Verification
- `cargo xtask affected-checks`, then focused:
  `./run.sh test -p ersatzrs-infra addon_media_storage`,
  `./run.sh test -p ersatzrs-scanner remote_stream_playlist` and the scanner
  `reidentified` test.
- `cargo clippy -p ersatzrs-infra -p ersatzrs-scanner --all-targets -- -D warnings`, `cargo fmt --all -- --check`.
- `tests/e2e/remote-streams.spec.ts` uses a hand-written sidecar with
  `one.yml`. Check whether it asserts on filenames after a sync and run only
  that spec if it does.
- Manual: sync a real yt-dlp list with an existing profile and check the
  managed folder (`ls`). Every `{id}.*` should have become `{title}-{id}.*`.
  Then check playout/EPG still show the same items (MediaItem ids unchanged).
- Cross-review with Codex before reporting done.
