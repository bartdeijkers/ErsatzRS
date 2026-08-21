# Add-on metadata management

## Goal

Import Beeld & Geluid, yt-dlp, and future add-on lists through one reviewable
metadata draft, then publish deterministic NFO, artwork, library projections,
and playlist membership only after operator approval.

## Delivery order

1. **AMM-A — contract and import repair.** Generate valid parent NFO metadata
   from the list header, accept ADR 0075, and introduce the shared contract.
2. **AMM-B — draft and publication.** Persist provider snapshots, overrides,
   inclusion, validation, conflicts, and atomic publication revisions.
3. **AMM-C — shared editor and artwork.** Use one localized editor for global
   and item metadata, with candidate selection, role assignment, upload, and
   safe URL acquisition.
4. **AMM-D — official providers and platforms.** Populate the same contract in
   Beeld & Geluid and yt-dlp and prove Linux, WSL, and Windows equivalence.
5. **AMM-E — refresh and closeout.** Merge provider refreshes per field,
   complete projection and health coverage, and run the full parity gates.

## Locked behavior

- Initial imports and newly discovered refresh items require publication
  review. Items default to included but warnings are prominent and filterable.
- Excluded items retain provider identity, metadata, artwork choices, and
  overrides but produce no library or playlist surface.
- Operator overrides win per field; provider changes beneath them are review
  conflicts. Unmodified fields continue to refresh.
- Existing lists migrate as published and included. Existing valid user-owned
  NFO data becomes operator-owned effective metadata without silent loss.
- NFO and artwork are generated outputs. Provider-specific parsing, keyword
  tables, and fixtures never enter host crates.
- The updateable `yt-dlp` binary does all YouTube-compatible discovery and
  metadata extraction. Its Deno helper is a bounded JSON-to-v4 mapper shared by
  the POSIX and PowerShell launchers; it must not become a second scraper.

## Verification

Focused contract, infra, scanner, web, health, migration, and browser tests run
per milestone. Closeout additionally requires both Clippy configurations,
`./run.sh test-closeout`, the consistent Leptos build, web parity check,
progress validation, native Linux/WSL POSIX package proof, and native Windows
PowerShell package proof.
