# Plan: make the EPG zoom-centre assertion real (and fix what it was hiding)

## Context

`tests/e2e/channel-watch.spec.ts:297` ("both surfaces autoplay muted and share
localized HLS failures") failed in CI twice with a byte-identical
`2.1666666666666288`, and went green only after FFmpeg 9 was installed at
`~/.local/ffmpeg-9.0`. That is a masking, not a fix — nothing about an FFmpeg
runtime should decide whether a zoom assertion holds.

Investigating it turned up the reason it cannot be reproduced locally: the timeline
element the test measures, `[role="grid"]` (the `.scroll` div in
`guide.module.css`, which carries `overflow: auto` and is meant to be the guide's
horizontal scroll container), measures **`clientWidth == scrollWidth == 9712px`**.
It is as wide as its own content, so it never scrolls. Consequences:

- the assertion is trivially satisfied — with `scrollLeft` pinned at 0, the computed
  "centre" is just half the viewport, which scales exactly with zoom, so the
  comparison passes for any window size and proves nothing; and
- the `Now` / `-2h` / `-1h` / `+2h` / `+4h` buttons (`guide.rs:85-93`, all routed
  through `scroll_to_minutes`) have nothing to scroll — a real user-visible defect.

Both host pages are shaped the same way: `.content` (`watch.module.css:88`) and
`.epg` (`epg.module.css:2`) are `display: grid` with **no `grid-template-columns`**,
so the single implicit track is `auto` and grid items keep the default
`min-width: auto`. The layout shell above them is already correct
(`layout.module.css` `.content-col` has `min-width: 0`), which is why the blow-out is
clipped rather than visible as an obviously broken page. The EPG route's own zoom
assertion (`~:487`) passes for the same hollow reason, so it is in scope too.

Outcome: the guide timeline becomes a genuinely scrollable container on both routes,
and the assertions covering it fail if it ever stops being one.

---

## Step 1 — Diagnose before changing CSS (required)

Measure, do not assume. Boot the app (`./run.sh serve`, or the e2e harness on port
18409) and on **both** `/epg` and `/channels/<id>/watch` walk from `[role="grid"]` up
to `.main`, recording `clientWidth` / `scrollWidth` / `offsetWidth` per ancestor.
Identify the first ancestor whose content width exceeds its parent's client width.

The implicit `auto` grid track on `.content` / `.epg` is the leading suspect, but the
fix must target whatever the measurement actually implicates.

## Step 2 — Fix the containment chain

Apply the minimal fix the diagnosis points at, most likely
`grid-template-columns: minmax(0, 1fr)` on the offending grid container(s):

- `crates/ersatzrs-web/src/admin/pages/channels/watch.module.css` → `.content`
- `crates/ersatzrs-web/src/admin/pages/channels/epg.module.css` → `.epg`
- possibly `crates/ersatzrs-web/src/admin/pages/channels/guide.module.css` → `.guide`
  (`min-width: 0`)

This is a user-visible layout change: check the watch and EPG pages at desktop,
tablet and mobile widths before/after.

## Step 3 — Make the assertions capable of failing

In `tests/e2e/channel-watch.spec.ts`, shared by the watch test (~:297) and the EPG
test (~:487) — fixing the helpers fixes both:

- **Add a precondition**: assert `scrollWidth > clientWidth` on `[role="grid"]` before
  the scroll/zoom proof. This is the change that makes the assertion real, and is
  exactly what would have caught the masked failure.
- **Stop hard-coding `352`** in `timelineCenterMinutes`. Read the sticky column width
  from the rendered `[role="columnheader"]` (`.corner`, `width: 22rem` in
  `guide.module.css`) so the test cannot silently desync from `CHANNEL_COLUMN_PIXELS`
  (`guide.rs:15`).

## Step 4 — Regression coverage for the real defect

Add assertions that the guide actually scrolls, covering the buttons that are
currently no-ops:

- `scrollWidth > clientWidth` on both `/epg` and the watch route;
- clicking `Now` / `+2h` changes `scrollLeft` in the expected direction.

## Step 5 — App-side hardening (only if the diagnosis calls for it)

`update_zoom` (`guide.rs:436-463`) reads geometry, then applies the scroll inside a
`request_animation_frame`, clamping `next_scroll` against a `scroll_width` that may
still reflect the pre-zoom layout. Once the element genuinely scrolls, that clamp
becomes reachable and could reintroduce drift. If the measurements show it biting,
recompute the clamp after layout settles. Do not change `zoomed_scroll_left` — its
centre-preserving maths is correct.

---

## Verification

Inner loop (keep one cargo configuration):

```bash
cargo xtask hydration-lint -p ersatzrs-web
./run.sh test -p ersatzrs-web
```

Browser proof — `~/.local/ffmpeg-9.0` is installed, so these run without the managed
download that previously perturbed the suite:

```bash
ERSATZRS_E2E_PORT=18409 scripts/e2e/playwright-e2e.sh channel-watch
ERSATZRS_E2E_PORT=18409 scripts/e2e/playwright-e2e.sh          # full 216-test suite
```

**Prove the new assertions actually bite**: temporarily revert the Step 2 CSS fix and
confirm the Step 3/4 assertions fail. An assertion that cannot fail is what caused
this problem in the first place — this check is the point of the whole slice.

Closeout gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings
./run.sh test-closeout
cargo xtask progress check
```

Bookkeeping: copy this plan to `plans/epg-timeline-scroll-container.md` in the repo
(plans are checked in, not left in the harness scratch file) and link it from
`plans/INDEX.md`; add a `CHANGELOG.md` `[Unreleased]` → `Fixed` entry for the
timeline; run `cargo xtask progress add-session`. If the EPG surface has a parity
file, record the fix there (`cargo xtask progress parity --feature <slug>`).

---

## Out of scope

- The missed CI runs on `main` (pushes `0e3980a..8c78db7` and `..8e17b27` created no
  workflow run). Root cause was a GitHub Actions incident, not this repository:
  the incident opened 2026-08-06T15:22:49Z (impact: critical) and throttled webhook
  triggers to roughly 15%, so most pushes and pull requests did not start a workflow
  run, on both GitHub-hosted and self-hosted runners. `workflow_dispatch` kept
  working and was used to run CI manually. No repository change is warranted; a
  standing CI sentinel was proposed and deliberately deferred.
- `target/` is at 41 GiB, past the 40 GiB `cargo clean` threshold — worth doing before
  the full-suite runs, but unrelated to this work.
