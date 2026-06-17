# Remote Streams Browser Parity Slice

> **Status: Completed / historical.** Shipped (`remote-streams-browser` is
> `Ported`). Kept for context; live state lives in `docs/progress/`.

## Source Map

- ErsatzTV UI/pages: `$ERSATZTV_SRC/ErsatzTV/Pages/RemoteStreamList.razor` at `0d301df5e8512fdfccf3a611188d27ab1690ee31` exposes `/media/remote/streams` and `/media/remote/streams/page/{PageNumber:int}`.
- ErsatzTV components/view models: `Shared/MediaCardPager.razor`, `Shared/MediaCard.razor`, `Shared/LetterBar.razor`, `Shared/FragmentLetterAnchor.razor`, `Pages/MultiSelectBase.cs`, `Application/MediaCards/RemoteStreamCardViewModel.cs`, `RemoteStreamCardResultsViewModel.cs`.
- ErsatzTV validators: none for this route; the pinned source has no remote-stream create/edit page or save/delete validator. Remote streams are scanned from local YAML definitions.
- ErsatzTV handlers/services: `Application/Search/Queries/QuerySearchIndexRemoteStreams*`, `Application/MediaCollections/Commands/AddMediaItemToCollection`, `AddItemsToCollection`, `AddItemsToPlaylist`.
- ErsatzTV repository/query paths: `Infrastructure/Search/LuceneSearchIndex` query `type:remote_stream AND (...)`, `RemoteStreamMetadata` projection with `RemoteStream`, thumbnail artwork, media versions, and state.
- Side effects: selection can add remote streams to collections or playlists; per-card add-to-collection is supported. There is no upstream route-local create/edit/delete side effect for remote stream rows.
- ErsatzRS target modules: `crates/ersatzrs-web/src/admin/pages/media/remote_streams.rs`, `remote_streams.module.css`, `admin/routes.rs`, `admin/server_fns/media_browser.rs`, progress docs.
- Intentional deviations: no add/edit/delete editor is added because the pinned ErsatzTV source does not expose one. The prompt-requested editor fields (`Url`, `Script`, grouping, save/delete) remain visible below as non-upstream scope.

## Visual Source Map

- ErsatzTV layout containers: `MudForm` wraps a square `MudPaper` 64px top action strip and a full-height scroll area; content uses `MudContainer MaxWidth.ExtraLarge Class="pt-8"`.
- ErsatzTV action bars/buttons: normal mode shows query text on the left, centered pager range/chevrons, and Select All on the right; select mode shows selected count plus Add To Collection, Add To Playlist, and Clear Selection.
- ErsatzTV tables/forms/pagers/dialogs: no table or editor form; content is a wrapping media-card grid with add-to-collection dialog and add-to-playlist dialog from shared multi-select base.
- Width/alignment/spacing rules: heading starts inside the ExtraLarge container below the 64px action strip; divider follows the `Remote Streams` h5 heading; cards wrap left-to-right with remote-stream thumbnail placeholders.
- Reference screenshots and viewport sizes: `plans/references/ersatztv_screenshots/0030_media_remote_streams.png` at a wide desktop viewport.
- ErsatzRS shared primitives/CSS modules to reuse: media-card and letter-bar primitives plus modal/toast primitives; dedicated CSS preserves the route's top action strip and content shell geometry.
- Known visual deviations: ErsatzRS has its project app shell and icon set, not MudBlazor; geometry is matched rather than component implementation.

## Acceptance

- [x] UI fields/actions/routes/back-links match ErsatzTV.
- [x] Validation and error surfaces match ErsatzTV.
- [x] Persistence and side effects match ErsatzTV.
- [x] Critical save/action paths have SSR or browser-safe fallback.
- [x] UI layout has screenshot/bounding-box evidence against ErsatzTV.
- [x] Repeated defect classes have reusable tests or playbook coverage.
- [x] Parity docs, status, and active plan files are updated.

## ParityDebt

- Prompt-requested remote-stream editor:
  - Status: intentionally divergent
  - Evidence: pinned ErsatzTV source contains `RemoteStreamList.razor` and query/view-model paths only; repository create/update occurs through scanner/YAML, not an admin editor.
  - Owner/follow-up: add only if a later ErsatzTV source introduces a remote-stream editor route.
  - ADR/deviation link: none; this is source-faithful scope control.

## Verification

- [x] `cargo test -p ersatzrs-web --features ssr remote_stream`
- [x] `cargo xtask hydration-lint -p ersatzrs-web`
- [x] `E2E_DB_PATH=.tmp/remote-streams-e2e.db npx playwright test tests/e2e/remote-streams.spec.ts --project=chromium`
- [x] `cargo xtask parity-check web`
- [x] `cargo xtask progress check`
- [x] Browser visual check against `0030_media_remote_streams.png` with desktop/mobile screenshots in `test-results/`.
