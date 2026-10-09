# Locked add-on Sources design: OAD v0.1

Accepted and explicitly locked by the maintainer on 2026-10-09. The lock
authorizes this design reference, not production implementation or publication.
[ADR 0118](../../../../adr/0118-bundle-and-refresh-official-addon-repository.md)
owns the product direction. `oad-01` through `oad-04` in
[structured progress](../../../../progress/milestones.toml) remain Not started,
behind teletext `p9b-02` and video jukebox `p10b-02`.

## Frozen reference

`design-lock.json` pins the four HTML documents, shared CSS, browser verifier,
verification snapshot and three selected previews by SHA-256. Verify from the
repository root:

```bash
python3 docs/reference/ui/design-artifacts/addon-sources/verify_lock.py
```

| Reference | Locked role |
|---|---|
| `index.html` | First run: official source and automatic repository updates On; unknown sources Off |
| `files.html` | Visible single ZIP picker after enabling unknown sources |
| `states.html` | Numbered loading, error, opt-out, confirmation, package-review and recovery states |
| `light.html` | First-run layout in the light theme |
| `styles.css` | Scoped hierarchy, responsive layout, control styling and color roles |
| `previews/` | Dark/light desktop and mobile file-installation references |

The administrative content and task hierarchy are the design reference.
Prototype review chrome, static application navigation, example data and
scenario links are excluded from production. Existing application shell and
routes remain authoritative. Label updates from Unlocked to Locked only record
the approval; they do not change the accepted layout or interaction proposal.

## Adaptation boundary

- Keep official discovery first and both ZIP kinds visibly supported through
  validated type detection and contextual package review.
- Preserve the repository-only update policy and persisted operator choices;
  source enablement does not authorize automatic yt-dlp installation.
- Retain labelled subordinate Git details, clear source-removal consequences,
  recovery messages and readable responsive controls.
- Replace synthetic values and links with localized DTOs, native forms and
  narrow hydration behavior using the existing app/runtime boundaries.
- Static confirmation panels do not provide functioning modal, persistence,
  upload or network behavior. Production must meet OAD-03 acceptance.
- State S16 explicitly leaves the detailed existing-profile adoption flow open.
  Locking that placeholder does not settle migration policy, update cadence,
  transport verification or other open implementation details in the plan.

The lock is not a WCAG, native-runtime or production acceptance claim. The
retained verification snapshot proves synthetic browser checks only.

## Later revisions

Do not edit the files listed in the manifest or overwrite their previews and
verification snapshot. The browser verifier writes disposable output under
`test-results/oad-design/`, so rerunning it does not update frozen evidence.

For refinements, start a new sibling artifact revision, retain this baseline,
record the changes and obtain explicit maintainer acceptance before changing
which revision OAD-03 uses. Review guides and progress notes may be updated to
point to a newer accepted revision without rewriting this locked history.
