# Add-on Sources design workspace

## Status

**Locked v0.1 — 2026-10-09.** The maintainer explicitly approved this reference;
[LOCK.md](LOCK.md) and `design-lock.json` define its frozen files. This is a synthetic HTML/CSS design
reference for OAD, not production UI or evidence that automatic updates exist.
The maintainer requested iterative visual review while teletext and jukebox
implementation continue. No implementation milestone starts through design work.

The accepted product direction is owned by
[ADR 0118](../../../../adr/0118-bundle-and-refresh-official-addon-repository.md).
[The plan](../../../../../plans/official-addon-distribution.md) owns sequencing;
`oad-01` through `oad-04` in [progress](../../../../progress/milestones.toml)
own acceptance. Both `p9b-02` and `p10b-02` must close before implementation.

## Preview

From the repository root:

```bash
python3 -m http.server 8417 --bind 127.0.0.1 \
  --directory docs/reference/ui/design-artifacts/addon-sources
```

Open <http://localhost:8417/index.html>. Direct file opening also works because
these pages have no scripts, external dependencies or backend requests.
Stop the preview server with Ctrl+C when finished. The preview port is separate
from the application's usual port; do not run a Leptos build to review this work.

| Artifact | Review purpose |
|---|---|
| [First run](index.html) | Official source available, automatic updates On, unknown sources Off |
| [Install from file](files.html) | Unknown sources enabled; one proposed picker for both ZIP kinds |
| [States and decisions](states.html) | Loading, failures, opt-out, trust, both package reviews, success, removal and adoption |
| [Light theme](light.html) | Same first-run layout with proposed light tokens |

Static previews: [desktop dark](previews/desktop-dark.png),
[desktop light](previews/desktop-light.png), and
[mobile file installation](previews/mobile-files.png).

The top review strip is prototype chrome and never belongs in the production
page. Application navigation outside Sources is static context. Scenario links
navigate to explicit states rather than executing operations. Native switches
can be toggled for keyboard/visual review but do not persist; the file picker
never reads or uploads contents. Modal state panels show content only, not focus
trapping or Escape behavior. The CSP prohibits scripts, connections and forms.

## Review in small decisions

1. Review the first-run hierarchy: can you identify the official source, its
   update policy and file installation immediately?
2. Review one task at a time: disable automatic updates, install an add-on ZIP,
   add a repository ZIP, recover from a failed check, or remove a source.
3. Refine wording, action emphasis and density only after the flow is clear.
4. Check the chosen design at 320/390 pixels, desktop, 200% zoom and in both
   themes. Use Tab and Space as well as the pointer.
5. Ask for a new numbered draft in a sibling artifact directory and an updated
   decision log; preserve the locked v0.1 bytes. Ask for two
   alternatives only when there is a specific unresolved tradeoff; name a
   preferred option before further polishing.
6. When satisfied, explicitly request a lock for the new revision. Record its
   accepted files/checksums and link its `LOCK.md` from OAD-03.
   Locking the artifact does not start implementation or authorize publication.

An effective feedback request identifies **page/state + desired outcome +
constraint**. Screenshots with annotations are useful, but precise text is enough:

> Create a design-only OAD v0.2 draft from locked v0.1. On First run, make Install from file easier to find
> without reducing the official source's priority. Keep both ZIP types and the
> default update setting. Show one revised draft and check narrow/light views.

For behavior decisions:

> Review states S07, S09 and S10. I want a shorter ZIP flow while still seeing
> which package I am installing and what access it requests. Propose the smallest
> change, then reflect my chosen option in the artifact and decision log.

## Work alongside runtime implementation

Use a separate design conversation and isolated worktree when another session
is modifying teletext, jukebox or release code. Assign design ownership to the
new sibling revision directory, for example
`docs/reference/ui/design-artifacts/addon-sources-v0.2/`, only. Keep shared progress,
ADR, changelog and runtime files owned by the integrating session during ordinary
visual revisions. Explicit product decisions can then be merged into those
records as a coherent checkpoint. Preserve unrelated files; do not run Git
cleanup or whole-tree staging to move the artifact between worktrees.

Suggested opening prompt:

> Start an OAD v0.2 draft from locked v0.1 in an isolated
> worktree. Design only; own the new revision directory. Read ADR 0118
> and the plan as constraints. Keep teletext/jukebox production work untouched.
> Start by showing the current draft, then apply this feedback: …

The current thread can refine the artifact too. A separate session/worktree is
the useful boundary when runtime coding is occurring simultaneously; it avoids
two editors modifying shared trackers or build artifacts.

## Interaction and accessibility specification

The primary operator goal is occasional setup and ongoing repository freshness,
with deliberate package execution. The shortest official path is first run →
Browse → existing install review. The local path is visible file installation →
trust confirmation if needed → identify/validate ZIP → appropriate package review
→ install or add repository → contextual next action.

- Use the existing settings/add-on navigation and application shell. The reduced
  desktop shell in this artifact is context, not an approved global redesign.
- The official source region owns a named switch, clear last-check/version facts
  and manual check. Pending, saved and failed settings states need announcements;
  persistence failures restore the actual saved value.
- A fresh profile is not labelled Up to date before an online check. Offline
  availability and successful remote freshness are independent facts.
- File installation remains outside disclosures. Trust confirmation is required
  only when enabling unknown sources; do not repeat it on every selection.
- One picker with validated type detection is the initial design proposal. Both
  supported ZIP formats remain required if review favors two inputs instead.
- Manual Git/storage details use the existing native disclosure pattern. The
  server path is labelled read-only; it is not a configurable text field.
- Source removal explains installed-data preservation and offers cancellation.
  Production uses a dialog with JavaScript and equivalent confirmation without
  it. The static state board does not claim working dialog semantics.
- Use semantic labels, `aria-current`, status/alert messages and focus rings.
  Normal text targets 4.5:1; essential controls and focus indicators target 3:1.
  Primary touch actions target 44 pixels. Content wraps at 320 pixels without
  page overflow. Long localized copy is a required production input.
- No motion is needed. Production strings use every supported culture; the
  English draft defines meaning and does not introduce hard-coded runtime copy.

## Implementation map

| Artifact element | Existing production boundary | Required adaptation |
|---|---|---|
| Settings/add-on navigation | `SettingsRouteTabs`, add-on manager route links | Preserve routes and active-page names |
| Action hierarchy | `Button`, `LinkButton`, existing form controls | Use semantic roles and existing tokens with measured contrast |
| Source status/refresh | `AdminResourceState`, app source services | Stable pending/error wrappers, persisted state and announced outcome |
| Trust/removal review | `Modal`, `DeleteDialog`, native confirmation endpoint | Contextual consequences, focus return and no-JS parity |
| Advanced Git details | Native section disclosure candidate | Label the server location; keep file installation outside |
| Repository panel + policy | Proposed composite, not a new generic primitive | Page-local composition until another consumer establishes reuse |

The eventual Rust mapping stays inside the narrow add-on management island and
app ports. Native forms must execute the same use cases. Repository calls,
validation and update jobs stay out of view code. Scope CSS changes to the page
unless a shared token correction has broader verified acceptance. OAD-03 owns
the production localization, keyboard, screen-reader, responsive and browser
evidence; synthetic markup cannot establish hydration or backend correctness.

## Decisions and open questions

| Revision | Decision / question | State |
|---|---|---|
| v0.1 | Bundle official source; enable source and automatic repository refresh on first run | Accepted direction, ADR 0118 |
| v0.1 | Preserve visible individual/repository ZIP installation | Required |
| v0.1 | Put manual Git setup under a labelled disclosure | Locked design |
| v0.1 | Use one ZIP picker with review after type detection | Locked design |
| v0.1 | Automatically install/enable yt-dlp | Not accepted; repository-only baseline |
| v0.1 | Exact adoption screen and refresh cadence | Resolve before their implementation slices |
| v0.1 | Hierarchy, density, colors and action wording | Locked; localize during implementation |

## Verification

Run `node docs/reference/ui/design-artifacts/addon-sources/verify.cjs` with the
preview server running. It uses the existing Playwright dependency and writes
disposable reports/screenshots under `test-results/oad-design/`. The checks cover
both browsers, reflow, semantic names, local link targets, keyboard switching,
disclosure access and calculated text contrast; they do not prove WCAG
conformance, native file-dialog behavior, screen-reader announcements, production
localization, hydration or persistence. Keep screenshots free of real profiles.

The milestone evidence records the actual verification result. Do not treat
the design lock as production acceptance.

The retained [v0.1 verification snapshot](verification.json) records 66 passing
Chromium/Firefox checks and the tested HTML/CSS hashes. Keep this snapshot and
its previews unchanged; a new revision owns new evidence. Browser zoom, assistive technology and real
production workflows still require their acceptance checks; the synthetic pass
uses narrow CSS viewports plus a separate 200% text-enlargement check.
