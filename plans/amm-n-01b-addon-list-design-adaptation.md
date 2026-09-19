# Remaining AMM-N-01B production acceptance

Milestone `amm-n-01b` in [the tracker](../docs/progress/milestones.toml) owns
status, acceptance and dated evidence. Local implementation is recorded there;
this plan retains outstanding production acceptance without claiming completion.
ADR 0091 owns the locked design and its recorded amendments.

## Remaining work

1. Complete [workspace refinement testing](addon-list-workspace-ux-refinements.md),
   retaining phase 4 and the editable numbering-review difference as outstanding.
2. Compare the real workspace against the
   [locked reference](../docs/reference/ui/design-artifacts/admin-ux-organize/LOCK.md), using its source/capability map.
   Do not infer missing production capability from a synthetic omission.
3. Map existing evidence to every milestone clause; investigate only missing
   proof or demonstrated failures. Retain real shell, localization, native forms,
   draft/revision fences, source/recovery eligibility and full audio/subtitle,
   alternate-track and chapter information.

## Evidence collection

The milestone owns required acceptance. Run builds/media checks serially with
bounded concurrency and the configured target-dir. Prototype outcomes under `amm-n-01b`
in `docs/progress/milestones.toml` are only design evidence.

Start with `cargo xtask affected-checks`, focused web/application/repository
tests, affected-crate Clippy, localization and hydration lint. Extend
`tests/e2e/addon-metadata-editor.spec.ts`, `addon-list-options.spec.ts` and
`addon-list-disk-warnings.spec.ts`; add a focused task-navigation spec where
useful. Assert real persisted state and revisions, not just visible labels.

Build a matched SSR/WASM/CSS bundle using BUILD.md's supported workflow. Run
Chromium/Firefox hydrated and JavaScript-disabled cases on temporary database
and provider fixtures. Keep screenshots, browser errors, state assertions and
bundle identity under `test-results/amm-n-01b/`. Cover:

- Empty/importing/ready/dirty/published/blocked/unavailable/recovery states;
  localized long titles, missing metadata, 165-item and larger generated lists.
- Both themes at 320, 390, 720, 1024, 1440, 1920 and 3840 CSS pixels; principal
  tasks at mobile/desktop with keyboard and 200% zoom. Check overflow, focus,
  disabled navigation, 44px principal controls, names/errors and contrast.
- Save/Discard, task/artwork/item navigation, chapters, every page size,
  off-page selection, numbering scopes, undo and inheritance. Saved-change publication and retry must
  retain newer unsaved input. Inject invalid/stale/write-failure cases.
- Actual source/revision/recovery eligibility and durable outcomes, repeated
  submissions and concurrent refresh; access to full audio/subtitle information.

Run `cargo xtask parity-check web` once for final route/shared-UI acceptance.
Update route/hydration trackers and TEST.md coverage as applicable. Run playback
smoke and applicable TSDuck checks if commands/persistence affect publication
or playback; do not call those changes CSS-only. N-03 owns the recorded Linux x64/Windows x64,
restart and real-channel gates; its platform amendments remain authoritative. Do not substitute prototype evidence.
