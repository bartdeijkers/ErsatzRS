# Rebuild the Add-ons Sources view

## Context

`/settings/addons?view=sources` reads as a stack of unrelated boxes. It carries
four blocks of equal visual weight — a risk toggle, a routing preference, an
unlabelled upload toolbar, and the repository list — with no grouping, no page
story, and a visual language (locally defined cards, badges, buttons) that
matches no other admin page. The operator cannot see the one fact that matters
for a source list: whether a catalogue is fresh, stale, or never fetched. That
data is already loaded and thrown away.

The maintainer asked for a critical UX/UI review and a plan. Decisions taken
during the review: the repository list becomes a **table in the media-sources
house style**, the **Default remote stream processor moves to the Installed
add-ons tab**, and the scope stays the **Sources tab** (plus that one moved
block).

## What is wrong today

Read against `crates/ersatzrs-web/src/admin/pages/addons.rs:394-520` and
`addons.module.css`.

**Structure**
1. Four blocks stack with identical weight; nothing states what "Sources" is
   for or which block is the primary one.
2. "Default remote stream processor" is a routing preference over *installed*
   add-ons, not a trust/provenance setting. It sits between the risk toggle and
   the repository list and pushes the actual list below the fold.
3. The upload row mixes three unrelated actions on one unlabelled line: install
   an add-on package, install a repository package, add a repository by hand.
   Two identical green **Install** buttons sit side by side with nothing saying
   which installs what.
4. `Add-on repositories` is a bare `h2` *below* the toolbar that acts on it, so
   the toolbar looks unattached to its list.

**Visual system**
5. The page re-implements buttons, badges, and cards in `addons.module.css`
   instead of using `ui/primitives/{button,badge,card,form}.rs`, so it looks
   unlike `settings.rs` (section title + divider + field rows) and unlike
   `media_sources/*` (toolbar + table + row actions).
6. Button hierarchy is flat: three equal-weight buttons in the toolbar, and
   Edit/Disable/Refresh all identical secondary green in each row, so routine
   and rare actions read the same. The result is a page dominated by green
   blocks.
7. Risk colour is inverted: with unknown sources **enabled**, the *safe* action
   ("Disable") is painted danger-red while the risky *state* carries no marker.
8. Native file inputs render browser-locale chrome ("Bladeren… Geen bestand
   geselecteerd") inside an English UI and look like nothing else on the page.

**Content and feedback**
9. `AddonRepositoryRow` already carries `last_refresh` and `last_sequence`
   (`admin/server_fns/addons.rs:6-17`) and neither is rendered. Freshness is the
   headline fact of a source list.
10. No per-repository add-on count, although it is derivable from
    `data.available` grouped by `repository_id`.
11. Disabled buttons never explain themselves: Delete on the official
    repository, Refresh for an unverified repository while unknown sources are
    off, everything under deployment management.
12. `last_error` renders as a bare red paragraph, and the empty `<p>` stays in
    the DOM as a blank line when there is no error.
13. The public key — the entire basis of the trust tier — is never summarised in
    the row, not even as a fingerprint.

**Interaction**
14. The repository editor is an inline block wedged between toolbar and list:
    no title, no Add-vs-Edit distinction, no validation, no required marking,
    and the ID stays editable while editing (changing it silently targets a
    different source). `ui/primitives/dialog.rs` (`Modal`) exists and is already
    imported by this page for confirmations.
15. Opening the editor moves neither focus nor scroll position.
16. The header's **Check for updates** refreshes every enabled repository,
    which on this tab is ambiguous next to a per-row **Refresh**.
17. The delete confirmation names the repository *id*, not its name.

**Accessibility**
18. Sections are not labelled groups; the file `<label>` uses `aria-disabled`
    while the inner input is genuinely disabled; the always-present empty error
    paragraph is a stray node.
19. Heading order does not match the visual hierarchy: two card `h2`s precede
    the section `h2` that heads the list.

## Target layout

```
Add-ons                                      [Check for updates]
Only add repositories you trust; signed add-ons run as the ErsatzRS service user.

Installed add-ons (2/2)  Browse add-ons  Updates (2)  Sources (1)
────────────────────────────────────────────────────────────────

Unknown sources                              Enabled ⚠   [Disable]
──────────────────────────────────────────────────────────────────
Allow unsigned repositories and local packages. Add-ons run with the
ErsatzRS service user's access.

Add-on repositories                                [+ Add repository]
──────────────────────────────────────────────────────────────────
NAME                       TRUST                ADD-ONS  LAST REFRESH
Official ErsatzRS add-ons  Verified · Official        2  2026-08-19 21:14
  org.ersatzrs.addons.official
  https://bartdeijkers.github.io/ErsatzRS-addons/index-v1.json
                                   Refresh · Edit · Disable · Delete

Install from file                                                   
──────────────────────────────────────────────────────────────────
Requires unknown sources. Add-on package  [Choose file…] fixture.zip [Install]
                          Repository      [Choose file…] none        [Install]
```

Three labelled sections in the `settings.rs` house style (`section_title` +
`divider`), ordered by how often they are used: the risk gate first because
everything else depends on it, the repository list as the working surface,
manual installation last because it is the exception.

## Changes

### 1. Sources view restructure — `crates/ersatzrs-web/src/admin/pages/addons.rs`

Rewrite the `view_name == "sources"` section (currently lines 394-520) as three
sections. Keep `style:display` toggling — **never** `<Show>` inside an island
(TEST.md hydration rule 2) — and keep the `addon-repository-row` and
`addon-repository-editor` test ids that `tests/e2e/addons.spec.ts` drives.

- **Unknown sources**: section heading + divider, help text, a trust badge for
  the current state (`Enabled` warn / `Disabled` neutral, reusing the local
  `status_badge` helper) and one button whose variant follows the *action*
  risk: danger only when the click enables unknown sources.
- **Add-on repositories**: heading row with the primary **Add repository**
  button on the right (toolbar attached to its list), then the table below.
- **Install from file**: heading + divider, one help line naming the unknown
  sources requirement, then two labelled rows (add-on package, repository
  package), each with a file picker and its own Install button.

### 2. Repository table — same file plus `media_sources/shared.module.css`

Import the shared sheet the way `media/remote_stream_playlists.rs:16` already
does, and render the list as a table with `row_btn` / `row_btn_del` actions:

| Column | Content |
| --- | --- |
| Name | name + trust badge + `Official` badge; id and index URL as muted sub-lines |
| Add-ons | count from `data.available` grouped by `repository_id` |
| Last refresh | `last_refresh` formatted `%Y-%m-%d %H:%M`, or `Never` |
| Status | `Enabled`/`Disabled`; `last_error` rendered only when present |
| Actions | Refresh, Edit, Enable/Disable, Delete (danger) |

Every disabled action gets a `title` naming the reason: official repository,
deployment-managed, or unknown sources required. Drop the always-present empty
error paragraph. Add an empty state for zero repositories.

### 3. Repository editor into a `Modal`

Replace the inline `repository_editor` block with the existing `Modal`
(`ui/primitives/dialog.rs`), title `Add repository` or `Edit repository`
depending on whether an id was carried in, `confirm_label = Save`. Make the ID
field read-only while editing (it is the identity being edited, not a field).
Reuse `TextField` from `ui/primitives/form.rs` where its shape fits; keep the
key as a `<textarea>`. Disable Save until name, URL, and id are non-empty.

### 4. Move the default remote stream processor

Move the `data-testid="default-remote-stream-addon"` block out of the sources
section into the `view_name == "installed"` section, above the card grid, in the
same section style. It selects among installed add-ons, so it belongs where
those are managed. Update the note in `docs/progress/hydration.toml` that
currently places it in the Sources view.

### 5. Localisation

New keys in `admin/i18n.rs` plus all nine files under `admin/languages/`
(`de_de`, `en_us`, `es_419`, `es_es`, `fr_fr`, `it_it`, `nl`, `pl`, `pt_br`):
`LastRefresh`, `NeverRefreshed`, `ChooseFile`, `NoFileSelected`,
`EditRepository`, `OfficialRepositoryProtected`, `UnknownSourcesRequired`,
`NoRepositories`. Everything else reuses existing keys (`AddonRepositories`,
`UnknownSources`, `InstallFromFile`, `InstallRepositoryFromFile`,
`AddRepository`, `Enabled`, `Disabled`, `Verified`, `Official`, `Refresh`,
`Edit`, `Delete`, `Save`, `Cancel`). Timestamps stay absolute so no
pluralised relative-time function is needed in nine languages.

### 6. CSS

Trim `addons.module.css`: remove `.security_card`, `.source_actions`,
`.file_action`, `.repository`, `.repository_list`, `.repository_editor` once
they are unused, and add only what the shared sheet does not cover (section
heading rows, the file-picker row grid). The Installed/Browse/Updates views keep
their current styling — this slice does not touch them beyond the moved block.

## Verification

1. `cargo clippy -p ersatzrs-web --all-targets -- -D warnings`
2. `./run.sh test -p ersatzrs-web addons`
3. `cargo xtask hydration-lint -p ersatzrs-web`
4. `./run.sh dev` (or the already-running instance) and walk
   `/settings/addons?view=sources` in the browser: add a repository through the
   modal, edit it, refresh it, toggle unknown sources, confirm the disabled
   Delete explains itself, and confirm the default remote stream processor now
   renders on the Installed tab.
5. `npx playwright test tests/e2e/addons.spec.ts` — the existing spec drives
   `addon-repository-editor` and `addon-repository-row`; both ids survive.
   `tests/e2e/settings-i18n.spec.ts` asserts the `Add-on repositories` marker on
   this view in three languages, which the new heading keeps.
6. `cargo xtask progress check`, CHANGELOG entry under `[Unreleased] → Changed`,
   and a session entry.

## Out of scope

Installed, Browse, and Updates keep their local card/button styling; migrating
the whole page onto the shared primitives is recorded as follow-up work rather
than done here. No server-side or contract change: every fact the new table
shows is already loaded by `load_addon_overview`.
