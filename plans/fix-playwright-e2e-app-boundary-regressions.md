# Fix the Playwright E2E regressions from the admin application-boundary push

## Context

CI run [30132852456](https://github.com/bartdeijkers/ErsatzRS/actions/runs/30132852456)
(push of `b4fbb4e` "complete admin application extraction" + `436a940`
"preserve mutation failure semantics", 2026-07-24) failed the **Playwright E2E**
job: 7 failures / 163 passed. Every other job (Clippy, Test, Format, Cargo deny)
was green, and the preceding run (`d54ef75`) was fully green — so this is a clean
regression from those two commits.

The 7 failures collapse into **3 root causes**, and two of them are genuine
product bugs (not test drift):

| # | Failing spec(s) | Root cause |
|---|---|---|
| A | `content-variants.spec.ts:378` (chromium+firefox), `support-pages.spec.ts:98` (chromium; the spec is chromium-only by design) | `/system/troubleshooting/playback` renders only its error branch, because the new `list_stream_selectors` runtime hard-errors on a **missing directory** — and it looks in the **wrong directory** |
| B | `island-hydration.spec.ts:139` `/media/smart-collections/add` (chromium+firefox) | A toast pushed **during SSR** bakes a `<div>` into HTML that the client toast store cannot reproduce → Leptos marker-node mismatch → `RuntimeError: unreachable` aborts `hydrateIslands` |
| C | `remote-streams.spec.ts:167` (chromium+firefox) | The new `admin_result_view` wrapper inserts a `<section data-admin-resource>` **above** `section.content`, so the spec's untyped `locator('section').first()` silently retargeted to the unconstrained wrapper (1764px instead of 1536px) |

Intended outcome: CI green again, with the two product bugs fixed at the level
that makes them unrepresentable rather than papered over, and the one stale
locator pinned so the same wrapper insertion can't retarget it again.

---

## A. `/system/troubleshooting/playback` renders an error page

### Diagnosis

`b4fbb4e` moved stream-selector listing out of
`admin/server_fns/troubleshooting.rs` into the new runtime trait impl. The old
implementation (now dead-coded to `#[cfg(all(test, feature = "ssr"))]` at
`crates/ersatzrs-web/src/admin/server_fns/troubleshooting.rs:2150`) was
infallible:

```rust
let Ok(mut entries) = tokio::fs::read_dir(folder).await else {
    return selectors;   // empty Vec
};
```

The new one at `crates/ersatzrs-web/src/host/troubleshooting.rs:115-141` turns
**any** `read_dir` io error — including `NotFound` — into
`AdminTroubleshootingRuntimeError::unavailable`. That propagates:

1. `crates/ersatzrs-app/src/lib.rs:1383` — `?` maps it to
   `ApplicationError::Unavailable { operation: "load playback stream selectors" }`
2. `crates/ersatzrs-web/src/admin/server_fns/troubleshooting.rs:317` — the
   `#[server]` fn returns `Err`
3. `crates/ersatzrs-web/src/admin/pages/system/troubleshooting.rs:548` — the
   `Some(Err(error))` branch renders a bare red paragraph, so neither
   `Missing mediaItem query parameter` (`:603`) nor `render_playback_form`'s
   `input[name="preferredAudioTitle"]` (`:626`) / `select[name="subtitleId"]`
   (`:649`) is ever emitted.

The directory never exists, for **two** reasons:

* **Wrong path.** ErsatzTV: `FileSystemLayout.cs:191` —
  `ChannelStreamSelectorsFolder = Path.Combine(ScriptsFolder, "channel-stream-selectors")`.
  ErsatzRS's convention maps ErsatzTV `<AppDataFolder>/X` content folders onto
  `data_path("X")` (cf. `music_video_credits.rs` → `data_path("templates/music-video-credits")`),
  so the correct path is `data_path("scripts/channel-stream-selectors")`. The
  channel editor already reads exactly that
  (`crates/ersatzrs-infra/src/repositories/channel.rs:693-697`), but
  `host/troubleshooting.rs:116`, `host/troubleshooting.rs:700` and
  `crates/ersatzrs-infra/src/playback.rs:898` all drop the `scripts` segment.
  **The read and write sides of the custom-stream-selector feature have never
  agreed** — this is a latent functional bug, not just a test failure.
* **Not created at startup.** ErsatzTV creates it in `Startup.cs:360`
  (`directoriesToCreate`). `crates/ersatzrs-web/src/host/bootstrap.rs` seeds
  `resources_cache_folder`, `fonts_cache_folder` and
  `default_mpegts_script_folder` but never this one.

Parity also confirms the empty case is a first-class state, not an error:
`GetChannelStreamSelectorsHandler.cs` uses `LocalFileSystem.ListFiles`, which
returns empty when the folder is absent *or* on any exception
(`ErsatzTV.Core/Metadata/LocalFileSystem.cs:61-80`), and the ErsatzRS page
already handles it — `troubleshooting.rs:617`,
`stream_selectors_disabled = context.stream_selectors.is_empty()`.

### Changes

1. **`crates/ersatzrs-infra/src/file_system_layout.rs`** — add one canonical
   accessor next to `mpegts_scripts_folder()`, with a doc comment naming
   `FileSystemLayout.ChannelStreamSelectorsFolder` as the parity source:

   ```rust
   /// `<data>/scripts/channel-stream-selectors` — user-authored custom stream
   /// selectors. Mirrors `FileSystemLayout.ChannelStreamSelectorsFolder`
   /// (`ScriptsFolder/channel-stream-selectors`).
   #[must_use]
   pub fn channel_stream_selectors_folder() -> PathBuf { ... }
   ```

2. Point **all four** call sites at it, deleting the ad-hoc `data_path(...)`
   joins: `host/troubleshooting.rs:116`, `host/troubleshooting.rs:700`,
   `crates/ersatzrs-infra/src/playback.rs:898`, and
   `crates/ersatzrs-infra/src/repositories/channel.rs:693-697`. Grepping for
   `channel-stream-selectors` afterwards must return only the layout module.

3. **Restore infallible listing** in `host/troubleshooting.rs:115-141`: a
   missing folder yields `Ok(vec![])`. Reuse the shape already proven in
   `crates/ersatzrs-infra/src/repositories/channel.rs:854-858`
   (`let Ok(mut entries) = read_dir(..).await else { return Ok(names) };`).
   Keep a `tracing::debug!`/`warn!` for non-`NotFound` errors so a permissions
   problem is still visible in logs, matching `LocalFileSystem`'s
   `LogWarning` on `UnauthorizedAccessException`.
   *While here, align the filter with ErsatzTV:* `ListFiles` returns **every**
   file except names starting with `._`, whereas the new impl keeps only
   `.yml`/`.yaml`. Since the channel-editor reader (`list_file_names`) does not
   filter either, both readers should share the parity behaviour so the
   troubleshooting dropdown and the channel-editor dropdown can never disagree.

4. **Create the folder at startup** in `crates/ersatzrs-web/src/host/bootstrap.rs`
   alongside the existing `create_dir_all` seeding, mirroring
   `Startup.cs`'s `directoriesToCreate`. This is the parity behaviour and it
   gives users a discoverable drop location.

5. Add a focused Rust regression test asserting `list_stream_selectors()`
   returns `Ok(vec![])` for a non-existent folder (so the infallible contract
   can't silently regress again).

### Verified as *not* needing a change

`admin/server_fns/troubleshooting.rs:325-333` now hard-errors when
`get_media_item_info` finds no row, where the pre-commit code used
`unwrap_or_default()`. This is **not** the cause of either failure (the specs
seed a real movie) and it is closer to parity than the old silent default:
ErsatzTV's `PlaybackTroubleshooting.razor:380-383` redirects to the home page
when the media item can't be loaded, i.e. it also treats the page as unusable.
Leave it as-is; note the deviation (typed error vs redirect) in the parity file.

---

## B. SSR toast breaks island hydration on `/media/smart-collections/add`

### Diagnosis

`/media/smart-collections/add` is a `StaticSegment("add")` route
(`crates/ersatzrs-web/src/admin/routes.rs:416-423`), so `collection_id` is `0`
(`pages/media/smart_collections.rs:268-272`). `436a940` changed the island's
loader to surface failures through the toast channel
(`pages/media/smart_collections.rs:281-288`):

```rust
let initial: Resource<CollectionForm> = Resource::new_blocking(
    move || collection_id,
    move |id| async move {
        load_or_notify(get_smart_collection(id).await, toast).unwrap_or_default()
    },
);
```

`crates/ersatzrs-app/src/queries.rs:1086-1091` rejects `id <= 0` with
`ApplicationError::Validation`, so on the *add* route the loader **always**
fails. `Resource::new_blocking` runs on the server, so `load_or_notify`
(`ui/primitives/toast.rs:127-142`, new in `436a940`) pushes into the
request-scoped server store *before* `ToastContainer` is serialized
(`layout/mod.rs:51` → `:109`, after `children()` in both DOM and flush order).

`use_toast()` (`ui/primitives/toast.rs:102-123`) is deliberately asymmetric — on
wasm it returns a fresh process-wide store, on the server the context store — so
at hydrate time the client sees `toasts.get() == []` while the DOM contains one
`<For>` child. Leptos walks to where the `For` marker should be, finds the
server-rendered toast `<div>`, and reports:

```
A hydration error occurred while trying to hydrate an element defined at
crates/ersatzrs-web/src/admin/ui/primitives/toast.rs:169:10.
The framework expected a marker node, but found this instead: JSHandle@node
RuntimeError: unreachable executed  at hydrateIslands
```

`toast.rs`'s own `view!` is byte-identical before/after — it is the victim, not
the culprit. Worse, because `Resource` loaders don't re-run on the client, an
SSR-time `load_or_notify` toast is *never* visible to the user anyway: the
channel is broken in both directions.

This violates `TEST.md`'s hydration rule 2 ("stable DOM shape") and its
resource-primitive rule ("server-backed resource primitives must keep the same
outer element across loading, ready, empty, and error states") — the intended
channel for a failed server-backed load is `admin_result_view` /
`AdminResourceState`.

Sibling editors already guard: `pages/media/collections.rs:288`
(`match (id != 0).then_some(id)`) and `pages/media/multi_collections.rs:261-282`
(`if id == 0 { default } else { … }` + `admin_result_view`). Smart collections
was the one that didn't, which is why it is the only `ISLAND_ROUTES` entry that
trips.

### Changes (full systemic guard, per decision)

1. **Make an SSR toast structurally impossible** —
   `crates/ersatzrs-web/src/admin/ui/primitives/toast.rs`: gate `ToastStore::push`
   so it is a no-op on non-wasm targets, documenting why (the client store
   cannot reproduce server state, so any server-rendered toast is a guaranteed
   hydration mismatch). This is safe for every existing caller: all 249
   `toast.push` / `load_or_notify` sites that matter run from `Action`
   callbacks or `Effect`s (client-only) — including `StartupNotifications`
   (`layout/mod.rs:65-107`), which pushes exclusively from inside an `Effect`.
   Extend `toast.rs`'s existing `mod tests` with a case asserting a
   server-side `push` leaves `toasts()` empty.

2. **Guard the add route** — `pages/media/smart_collections.rs:283-288`: when
   `collection_id == 0`, return the default `CollectionForm` without calling
   `get_smart_collection`, matching `collections.rs:288`. Do the same for the
   editor's other loader if it shares the pattern.

3. **Route real load failures to the stable surface** — for a non-zero id whose
   load fails, render through `admin_result_view`
   (`ui/primitives/resource_state.rs:76-101`) as
   `MultiCollectionEditorPage` does, so the failure keeps 436a940's visible
   semantics while emitting the durable
   `[data-admin-resource][data-state="error"]` surface that
   `island-hydration.spec.ts:219-225` already asserts on. This means hoisting
   the async boundary into `SmartCollectionEditorPage` and passing the loaded
   form into the island as a stable prop.

4. **Add `cargo xtask hydration-lint` rule-3** —
   `xtask/src/hydration_lint.rs`. The existing file is a `syn`-based AST lint
   with rule-1 (`<Show>` in an island) and rule-2 (prop-bearing island across
   `Suspense`), a `(file, rule)` baseline mechanism (`load_baseline`, `:141`),
   and a `report()` helper (`:377`). Add rule-3: flag `load_or_notify(..)` or
   `<expr>.push(..)` on a toast store appearing inside the loader closure of a
   server-executing `Resource::new` / `Resource::new_blocking`
   (`LocalResource` and `Action`/`Effect` bodies stay allowed). Message should
   point at `admin_result_view` as the correct channel. Run the lint over the
   tree and, if it surfaces the latent siblings the audit flagged
   (`collections.rs:289`/`:414`, `watermarks/editor.rs:155`,
   `media/browser.rs:68`, `media_sources/{plex,jellyfin,emby}.rs`), fix them
   the same way rather than baselining them.

5. Document rule-3 in **`TEST.md`**'s hydration-rules section next to rules 1-2,
   including the one-line "why" (server store ≠ client store).

### Also noted, out of scope

`crates/ersatzrs-web/src/admin/layout/mod.rs:44` —
`{theme_error.map(|error| view! { <AdminErrorState error/> })}` can only ever be
`Some` on the server, so `<main>` has a different child count SSR vs CSR. Harmless
today because `AppShell` is a `#[component]`, not an `#[island]`, so it is never
hydrated — but it is the same divergence class. Record it as parity debt rather
than fixing it in this change.

---

## C. `remote-streams.spec.ts` geometry locator retargeted

### Diagnosis

The geometry contract is **intact**; only the locator drifted.

* `tests/e2e/remote-streams.spec.ts:153` sets a 2048×1024 viewport;
  `:164` locates `page.locator('section').filter({ has: heading 'Remote Streams' }).first()`
  — the first `<section>` *containing* the heading, not a specific class.
* The intended target is
  `crates/ersatzrs-web/src/admin/pages/media/remote_streams.rs:109`
  `<section class=style::content>`, constrained by
  `remote_streams.module.css:150-154` → `width: min(100%, 96rem)` = **1536px**.
  1540 = 1536 + 4px tolerance, consistent with the sibling media pages'
  explicit `max-width: 1536px` (`collections.module.css:30`,
  `collection_detail.module.css:31`, `rerun_collections.module.css:24`).
  That CSS was **not** touched in this range.
* `b4fbb4e` wrapped the page in `admin_result_view`
  (`pages/media/remote_streams.rs:72-85`), which emits
  `<section class=resource_state data-admin-resource …>`
  (`ui/primitives/resource_state.rs:39-46`) whose only style is
  `min-width: 0` — no max-width. As a plain block it fills the shell's main box:
  `2048 − 224` (drawer, `layout.module.css:139`) `− 60` (main padding,
  `layout.module.css:31-33`) = **1764**, exactly the received value.
  Deterministic, and identical in both browsers — not flake.
* The new wrapper is an *ancestor* of `section.content`, so `.first()` now
  resolves to it. Nothing visually regressed; the card grid is still centred at
  1536px. `island-hydration.spec.ts:219-225` shows `[data-admin-resource]` is an
  intentional, load-bearing test surface, so the fix belongs on the locator.

### Changes

1. **`tests/e2e/remote-streams.spec.ts:164`** — pin the locator to the
   constrained container so an inserted structural wrapper cannot retarget it,
   e.g. `page.locator('section[class*="content"]:not([data-admin-resource])')`.
   `class*=` matching is the established convention in these specs
   (`div[class*="action-bar"]` at `:159`, `div[class*="toast-base"]`,
   `[class*="selection-count"]`, …) and works because stylance emits
   kebab-case names with a hash suffix.
2. **Fix the same latent bug where it is currently masked** (these pass only
   because they never widen the viewport past 1280):
   `tests/e2e/collections.spec.ts:79` and `:118` (same locator shape, same 1540
   bound; `collections.rs:45-47` also adopted `admin_result_view` in this
   range), and the looser
   `tests/e2e/media-collection-route-family.spec.ts:113-121`
   (`'section, form, div'`, bound 1600).
3. **`TEST.md`** — one line under the admin-UI test guidance: `admin_result_view`
   wraps page content in `section[data-admin-resource]`, so geometry assertions
   must pin the element they measure rather than using a bare tag selector.

---

## Contract updates (required by AGENTS.md)

* **`CHANGELOG.md`** `[Unreleased]` → `### Fixed`: (a) custom channel stream
  selectors are now read from the same ErsatzTV-parity folder the channel editor
  writes to, and a missing folder no longer breaks the playback-troubleshooting
  page; (b) the smart-collection add route no longer breaks admin-UI hydration.
  Note the folder-path correction explicitly — it is user-visible.
* **`docs/progress/parity/`** — update `admin-ui-failure-semantics.toml` (the
  SSR-toast hazard and the `admin_result_view` channel being the only safe one
  for server-backed loads), `collections-smart-collections.toml`,
  `remote-streams-browser.toml`, and whichever troubleshooting-playback feature
  file covers stream selectors. Record the `layout/mod.rs:44` divergence as
  `ParityDebt`, plus the pre-existing `scripts_folder()`
  (`data_local_dir()/ErsatzRS/scripts`) vs `data_path("scripts/…")`
  (`~/.ersatzrs/data/scripts`) root inconsistency as a flagged follow-up.
* Run `cargo xtask progress check`, then
  `cargo xtask progress add-session --agent "claude" --summary "…"`.

---

## Verification

Inner loop (keep **one** cargo configuration to avoid full rebuilds — the
target-dir/feature/`--all-targets` flip re-runs rustc over the tree):

```bash
./run.sh check -p ersatzrs-web
./run.sh test  -p ersatzrs-web troubleshooting
./run.sh test  -p ersatzrs-web toast
./run.sh test  -p ersatzrs-infra file_system_layout
cargo xtask hydration-lint -p ersatzrs-web     # must flag nothing after the fixes
```

E2E — build the SSR binary and WASM bundle **in lockstep** (`./run.sh serve`, not
a separate `cargo leptos build` + `cargo run`, or the wasm-checksum trap makes
every island look dead), then run only the four affected specs first:

```bash
npx playwright test tests/e2e/support-pages.spec.ts \
                    tests/e2e/content-variants.spec.ts \
                    tests/e2e/island-hydration.spec.ts \
                    tests/e2e/remote-streams.spec.ts
```

Then the two specs whose latent locator bug we fixed, at a wide viewport, to
confirm the masking is gone:

```bash
npx playwright test tests/e2e/collections.spec.ts \
                    tests/e2e/media-collection-route-family.spec.ts
```

Manual confirmation that A is really fixed independent of the tests:

```bash
rm -rf ~/.ersatzrs/data/scripts/channel-stream-selectors   # prove the empty case
./run.sh serve
curl -s http://127.0.0.1:8409/system/troubleshooting/playback | grep -c "Missing mediaItem"
# then re-check with a file dropped in the folder: it must appear in the
# troubleshooting Stream Selector dropdown AND in the channel editor's list
```

Closeout gate (run **once**, not per edit):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
./run.sh test-closeout
cargo xtask parity-check web
cargo xtask progress check
(cd crates/ersatzrs-web && cargo leptos build)
npx playwright test          # full suite: expect 170 passed / 12 skipped, 0 failed
```

Also mirror this plan into `plans/` in the repo before implementing, per the
project's plan-location convention.
