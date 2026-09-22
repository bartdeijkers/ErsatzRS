# Locked add-on list design: AMM-N-01B, revision 1

Accepted by the maintainer on 2026-09-09. Production adaptation is governed by
[ADR 0091](../../../../adr/0091-lock-addon-list-task-design.md) and the
[AMM-N-01B acceptance and evidence](../../../../progress/milestones.toml).

## Reference authority

`design-lock.json` pins every root HTML/CSS file, artwork asset, both Python
renderer modules and the selected current screenshots below. The rendered pages
and native task behavior are authoritative. Older reviews, screenshots, draft
skill guidance and earlier Apply/checkbox wording are historical context.

The lock covers `.design-main` and its task flow. Production keeps its real
admin shell, including ADR 0089's Support navigation. Preview banners, synthetic
drawers, scenario navigation and simulated completion controls are not production
UI. Account for the real shell when comparing content geometry.

Verify the baseline from the repository root:

```bash
python3 docs/reference/ui/design-artifacts/admin-ux-organize/verify_lock.py
```

Do not edit frozen files or overwrite reference screenshots during the port.
Run prototype browser checks only in a disposable copy: the existing scripts
write screenshots beside the artifact. Put production comparison evidence under
`test-results/amm-n-01b/`. Intentional design changes need a maintainer decision,
a new lock revision, updated hashes and a recorded explanation.

## Locked views and states

| Task | HTML reference | Current screenshot reference |
| --- | --- | --- |
| Items and publication summary | `index.html`, `large-list.html`, `long-list.html` | `screenshots/ux-fixes-{chromium,firefox}-{1440,390}-{dark,light}.png`; `screenshots/replacement-{chromium,firefox}-large-{1440,390}.png` |
| Expanded bulk groups | `index.html`, numbering/selection/range/preview scenarios | `screenshots/simplified-{chromium,firefox}-index-{1440,390}.png` |
| List settings | `settings.html` | `screenshots/simplified-{chromium,firefox}-settings-{1440,390}.png` |
| Playback/access | `options.html` | `screenshots/simplified-{chromium,firefox}-options-{1440,390}.png`; `screenshots/access-stack-{chromium,firefox}-{4k,390}.png` |
| Item review | `item-1.html` through `item-36.html` | `screenshots/replacement-{chromium,firefox}-item-{1440,390}.png` |
| Activity/recovery | `activity.html`, `recovery-list.html`, source/reconcile/restore states | `screenshots/replacement-{chromium,firefox}-recovery-{1440,390,320}.png` |
| Artwork and secondary tasks | `artwork.html`, `replacement.html`, `media-details.html`, `states.html` and linked scenarios | Locked HTML/renderer states; capture matched comparisons during implementation |

Braces denote filename alternatives; the manifest lists each concrete file.
The 4K access images are element crops from a 3840×2160 viewport. Historical
images outside the manifest are not acceptance baselines.

## Locked interaction and layout decisions

- Preserve four task destinations, heading hierarchy, restrained status styling,
  spacing, control sizing, aligned item columns and mobile records.
- Use Save settings, Save item and Save numbering. Publish saved changes operates
  on saved state; revision identifiers remain secondary.
- Keep Previous and Next in fixed slots, visibly disabled at boundaries. Return
  to the originating row and retain list context and drafts.
- Keep one reachable numbering Save/Discard bar; no duplicate action sets.
  Keyboard focus remains unobscured by the app bar and sticky controls.
- Provide page sizes 10, 25, 50, 100 and All; retain drafts and selection across
  paging/filtering and show selection totals and off-page counts.
- Keep one outer bulk editor with visible numbering and media/playback groups.
  Routine list/playback settings stay visible. Retain optional disclosures for
  credits, metadata/chapters, algorithm bounds and diagnostics; reveal errors.
- Stack Access and segments vertically at every width, with aligned labels,
  controls and saved-value explanations and a 48rem maximum group width.
- Use one default/explicit-value selector per access/segment setting. An explicit
  value equal to the default is still an override. Populate production choices
  from the installed provider schema, not a hard-coded browser list.
- Show actionable recovery outside diagnostics and feedback beneath the heading.
  Conflicts and blockers cannot disappear into optional content.

## Reference mapping

The [production source map](production-source-map.md) identifies the existing
owners and capability boundaries. Detailed historical prototype outcomes are
recorded under `amm-n-01b` in `docs/progress/milestones.toml`.

## Production adaptation boundary

Replace synthetic values/results with real DTOs and commands; localize all text.
Preserve icons, security, ETags, revision/generation fences, durable jobs and media
semantics. Additional existing fields belong in their matching locked task group;
synthetic omissions do not authorize removal. Full Remote Stream audio/subtitle,
alternate-track and chapter information remains reachable.

Python, browser scripts and reports prove only the prototype. They do not justify
shipping in-memory persistence or claiming production readiness. Milestone
`amm-n-01b` owns production acceptance.
