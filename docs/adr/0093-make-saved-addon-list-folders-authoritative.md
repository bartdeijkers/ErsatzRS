# ADR 0093: Make saved add-on list folders authoritative

## Status

Withdrawn (2026-09-13), previously accepted 2026-09-12

## Implementation

Not implemented — the maintainer stopped `amm-o-01` and restored pre-implementation
baseline `de14a03c`. Recovery checkpoint `0e9413d3` preserves the stopped work.
The decision below is historical and no longer governs implementation.
ADR 0075 retains database ownership of saved drafts and publication snapshots.

## Context

The maintainer wants to zip saved list folders, share them, and unpack them
under another installation's configured add-on media root. The next ordinary
scan or scheduled scan must discover those lists and update ErsatzRS without
copying SQLite or manually re-adding each source URL.

ADR 0068 already makes NFO descriptive authority and YAML operational authority
for scanned media. That does not establish complete folder-to-Add-on-List
reconstruction: ADR 0075 introduced database-owned drafts and publication
snapshots, and the storage reference's portable-subfolder wording has no
fresh-database acceptance proof.

## Decision

Saved add-on list content on disk is authoritative over its SQLite projection.
For yt-dlp, this includes list subfolders below the configured media root,
normally `<profile>/yt-dlp_media/`. The host behavior remains provider-neutral.

- A normal scan or scheduled scan discovers valid saved list folders, including
  folders copied from another installation, and creates or updates their list
  information in SQLite. No database transfer or manual URL re-import is needed.
- Valid saved list files win over stale database copies of that content.
  Administrative saves must persist the portable content, not only change
  SQLite. Reuse existing YAML/NFO and managed-file capabilities where sufficient.
  The plan must define concurrent-edit reconciliation: genuinely conflicting
  saved content is surfaced for review, not silently overwritten by a scan.
  SQLite does not become a competing authority for the same saved content.
- Portable identity and references must not depend on the sender's SQLite IDs
  or absolute profile path. Referenced list-owned files travel with the folder.
- SQLite remains local storage for runtime coordination, jobs, leases and
  derived indexes. Copying content does not copy active process ownership or
  install/authorize an add-on executable.
- Preserve explicit Save/Publish separation and current published playback.
  The focused implementation plan must define copied draft/publication handling
  and reconciliation with local unsaved work before changing those boundaries.
- Keep the existing per-list subfolder and `list.playlist.yml` model. Removing
  a list in the admin UI retains its saved folder and files but hides the list
  in the admin UI (maintainer decision, 2026-09-12). Subsequent ordinary or
  scheduled scans must not automatically make that hidden list visible again.
  Hiding is installation-local, not portable list content: sharing a folder
  does not transfer the sender's hidden state, so a newly discovered copy is
  visible on the receiving installation (maintainer confirmation, 2026-09-12).
  This decision concerns retention and visibility; it does not authorize new
  playback or unloading behavior. The focused plan must still settle how an
  operator can reveal a hidden list. No persistence mechanism is chosen here.

This supersedes database-only ownership of saved portable list content, not
the publication safety requirements of ADR 0075 or durable coordination in
ADR 0085. The current beta must not be described as implementing this decision.

## Consequences

List folders become shareable and their content can rebuild the corresponding
database projection. The next slice must inventory missing durable information
and provide the smallest complete reconstruction path. It must not grow into
database backup, arbitrary-folder synchronization, or a separate ZIP service;
the user can zip and unpack the ordinary folders themselves.

Copied ownership sidecars, artifact pins and machine-local access references
are not trusted runtime authority. Validate portable media and local bindings
without treating copied identifiers as permission to execute code or remove
files. Preserve the installed/enabled add-on boundary.

Acceptance needs a copied-folder test against an independent database/profile,
plus repeat-scan idempotence and stale-database reconciliation. Invalid or
incomplete files must be reported without replacing valid saved content.

## Alternatives Considered

- Require a SQLite backup or manually re-add every source URL: rejected because
  it does not provide the requested folder-sharing workflow.
- Treat files solely as replaceable database-generated output: rejected because
  a stale database would remain authoritative over the shared content.

## References

- [ADR 0068](0068-addon-metadata-sidecars-and-local-replacements.md)
- [ADR 0065](0065-managed-remote-stream-playlists.md)
- [ADR 0075](0075-review-addon-metadata-before-publication.md)
- [ADR 0085](0085-broaden-addon-list-lifecycle-and-provider-options.md)
- [Current delivery plan](../../plans/addon-list-lifecycle-broadening.md)
- [Structured milestones](../progress/milestones.toml)
