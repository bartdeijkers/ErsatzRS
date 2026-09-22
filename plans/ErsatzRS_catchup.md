# Plan: Catch ErsatzRS up with upstream ErsatzTV, one commit at a time

> Historical seed plan. The [live ledger](upstream-catch-up.md) and structured
> milestones govern execution. The maintainer's 2026-09-19 amendment replaces
> the fixed `495e3799` cutoff with the latest fetched upstream tip on every run
> (162 commits at the 2026-09-19 bootstrap; the live count is in the ledger and
> the pin document). Next-engine ADR references to 0094 below refer to
> ADR 0095; ADR 0094 owns automatic publication. The maintainer rejected the
> native-only proposal and accepted Next as an optional second engine on
> 2026-09-22. Next-specific exclusion hypotheses below are withdrawn; use the
> live ledger and accepted ADR for execution.
> Each task processes at most one upstream commit, including N/A rows.
> The 2026-09-20 decision requires direct substantive Codex review only;
> historical review instructions below do not govern the active procedure.

## Context

ErsatzRS measures parity against a fixed upstream snapshot:
`docs/automation/parity-baseline.md` pins ErsatzTV at `0d301df5` ("remove
external dependencies (bugsnag, trakt) (#2840)"). The `$ERSATZTV_SRC` checkout
is at that pin, and `origin/main` is **152 commits ahead**. That range covers
upstream releases v26.4.0 through v26.9.0, plus ten later commits up to tip
`495e3799`.

Upstream changed shape in that window. The repository was renamed "ErsatzTV
Legacy" (`1e5c0b12`). About 40 commits add or extend **ErsatzTV Next**: a
separate `ersatztv-channel` binary (from the `ErsatzTV/next` repo) that legacy
feeds with playout and channel-config JSON. The window also contains real
behavior work: scheduling fixes, subtitle handling, security fixes (API key,
streaming-port hole, GHSA-h3r4-r2f2-qf59, artwork authorization), Plex, Jellyfin
and local scanning fixes, YAML scheduled playouts, the graphics canvas, and
health checks.

The goal is to advance the pin **commit by commit** to `495e3799` while honoring
the ErsatzRS ADRs. Every upstream commit gets a recorded verdict. Behavior is
ported where it applies, and every deviation traces to an ADR.

### Maintainer decisions (2026-09-14)

1. **Next engine:** port the behavior, not the engine. A new ADR rules out an
   external streaming engine. User-visible behavior from Next commits that our
   own pipeline lacks is ported natively. Pure engine plumbing is not applicable.
2. **Priority:** after AMM-N-03 (Windows beta acceptance), before Phase 9A
   teletext (`p9a-01`).
3. **Security fixes:** strict order, with no out-of-order fast track.
4. **Granularity:** one ledger row, verdict and pin step per upstream commit.
   Consecutive not-applicable rows land together in one ErsatzRS commit.

---

## Step 0 — Set up the catch-up program (docs-only checkpoint, `ucu-00`)

1. **Live plan and ledger:** create `plans/upstream-catch-up.md` (per the repo
   plans-in-repo rule). Include this procedure and a 152-row ledger generated
   from `git -C "$ERSATZTV_SRC" log --reverse --format='%h|%s' 0d301df5..495e3799`.
   Columns:
   `# | upstream sha | subject (PR) | verdict | owning parity slug | ErsatzRS commit | notes`.
   Seed each row with the provisional triage below. The verdict is confirmed only
   when that row's step runs.
2. **ADR 0094 — "Native pipeline instead of the ErsatzTV Next streaming
   engine"** (via `skills/new-adr.md`). Status `Proposed` until the maintainer
   accepts it; Implementation `Not implemented`. It builds on ADR 0003 (spawn
   FFmpeg ourselves), ADR 0005 (no external graphics process) and the
   single-binary goal. Rules:
   - No per-channel `StreamingEngine` column, no `ersatztv-channel` spawn or
     bundling, and no Next playout or channel-config JSON export.
   - If a Next commit adds behavior legacy already had, and ErsatzRS has it too,
     the verdict is `Covered`.
   - If a Next commit adds behavior that ErsatzRS lacks, port it into
     `ersatzrs-ffmpeg`, the scheduler or the app layer, then record `Port`
     (ADR 0094).
   - Next-only UI (version display, Next troubleshooting) is `N/A (ADR 0094)`.
3. **Tracking record:** the milestone `feature` must resolve to a parity slug
   (`xtask/src/progress.rs:1203`). Add `docs/progress/parity/upstream-baseline-catch-up.toml`
   with status `In progress`, a summary pointing at the ledger, and ADR `0094`.
   Feature debt stays in the owning slugs, never here.
4. **Milestones** in `docs/progress/milestones.toml`, phase
   `UCU — Upstream ErsatzTV catch-up`. There is one milestone per upstream
   release window, chained with `depends_on`:

   | id | Pin target | Window |
   |---|---|---|
   | `ucu-00` | — | setup; `depends_on = ['amm-n-03']` |
   | `ucu-01` | `a74896df` | → v26.4.0 |
   | `ucu-02` | `290f991b` | → v26.5.0 |
   | `ucu-03` | `637e6be8` | → v26.5.1 |
   | `ucu-04` | `0b44ad47` | → v26.6.0 |
   | `ucu-05` | `35b5a259` | → v26.7.0 (second prep commit) |
   | `ucu-06` | `7b7a19a2` | → v26.8.0 |
   | `ucu-07` | `49b68314` | → v26.8.1 |
   | `ucu-08` | `5bb3ddfc` | → v26.9.0 |
   | `ucu-09` | `495e3799` | → tip |

   Add `ucu-09` to `p9a-01.depends_on`, and update `phase.toml`
   `current_focus`/`next_steps` so teletext follows the catch-up.
5. **Pin doc:** add a "Catch-up mode" section to `docs/automation/parity-baseline.md`.
   It names the target `495e3799`, links the ledger, and says the pin advances
   only through ledger steps (the daily routine still never advances it). The
   routine and `gap-prompt-rules.md` need no edits: they already measure against
   whatever the pin says.
6. **Commit authorization:** ask the maintainer for standing authorization to
   commit UCU steps. Recording a handoff does not authorize commits.
7. Run `cargo xtask progress check`, get a Codex cross-review, then commit
   `docs(progress): open upstream ErsatzTV catch-up program`.

---

## Per-commit procedure (repeat for the next unprocessed ledger row)

**Invariant:** `parity-baseline.md` pin equals `git -C "$ERSATZTV_SRC" rev-parse HEAD`,
which equals the last processed ledger row.

1. **Read upstream.** Run `git -C "$ERSATZTV_SRC" show --stat <sha>`, then read
   the full diff: code, upstream tests, and the CHANGELOG hunk. Read the
   surrounding files at that commit with `git show <sha>:<path>`. Keep the
   checkout itself at the current pin until the step lands.
2. **Map to ErsatzRS.** Use the PORTING.md critical-file table and the owning
   parity slug's `etv_source`/`rs_target`. Find the equivalent Rust code and
   check whether ErsatzRS already differs there.
3. **Check for ADR conflicts.** These ADRs bite most often in this window:

   | ADR | Consequence for upstream commits |
   |---|---|
   | 0094 | Next engine |
   | 0003, 0005, 0077 | FFmpeg spawn and graphics (drawvg/drawtext instead of Skia) |
   | 0004 | New sqlx migration in `migrations/`, never a copied EF migration |
   | 0080 | SQLite only: MySQL halves are N/A |
   | 0008, 0063 | FFmpeg 9 runtime: upstream ffmpeg 8.1.2 bumps and bundles are N/A |
   | 0009 | SQL search instead of Lucene |
   | 0051 | Application boundary |
   | 0052, 0056, 0061 | Subtitle and audio selection |
   | 0064 | HDR/Dolby Vision |
   | 0076 | Music video credits templates |
   | 0012 and locale ADRs | Every new UI string is added in all cultures |

4. **Pick a verdict.**
   - `Port`: implement the upstream behavior.
   - `Adapt (ADR nnnn)`: implement the behavior under an existing ADR.
   - `Covered`: ErsatzRS already has it; cite the code or test.
   - `N/A (ADR 0094 | 0080 | 0063)`.
   - `N/A (upstream tooling)`: CI, release prep, changelog, NuGet or Docker
     plumbing.
   - `Superseded by <sha>`: only inside a consecutive fix cluster in the same
     milestone (for example `e5a2a763`→`0d692e70`→`4fb42f69`). The final shape
     is ported at the last commit of the cluster; the pin still steps through
     each row.
   - **Stop and ask the maintainer** if a row needs a new ADR, a `Deferred`
     verdict, or any change to acceptance requirements.
5. **Implement (Port/Adapt).** Follow `skills/parity-slice.md`.
   - Port upstream C# tests as Rust tests at the narrowest level. Use `insta`
     snapshots for FFmpeg argv, filter graphs and YAML output.
   - For schema commits, use `skills/add-migration.md`.
   - Update the health check when runtime or persisted state changes.
   - Localize every UI string.
   - Add a CHANGELOG `[Unreleased]` entry that credits the upstream PR.
   - Update the owning `docs/progress/parity/<slug>.toml`.
6. **Verify, focused only.** Run `cargo xtask affected-checks`, then
   `./run.sh test -p <crate> <filter>` and `cargo clippy -p <crate> --all-targets -- -D warnings`.
   - Playback-sensitive rows also run `cargo xtask smoke`.
   - Browser-visible rows run the relevant Playwright spec and
     `cargo xtask hydration-lint -p ersatzrs-web`.
   - Pass `CARGO_BUILD_JOBS=8` and run builds serially (WSL out-of-memory
     history).
7. **Review.** Codex cross-reviews the diff, tagging findings P1 or P2. Fix
   every P1 before committing.
8. **Land.** One commit contains:
   - the change;
   - the ledger row filled in (verdict, slug, notes);
   - the pin row in `parity-baseline.md` set to `<sha>`/subject.

   Then advance `$ERSATZTV_SRC` to `<sha>`. Message format:
   `<type>(<scope>): <summary>` with body `Upstream: ErsatzTV <sha> (#PR)`.
   Runs of consecutive N/A rows use one
   `docs(progress): advance ErsatzTV baseline to <sha>` commit listing each
   skipped sha and its reason.
9. **Close the milestone** at the window's last row:
   - full gate: clippy, `./run.sh test-closeout`, `cargo leptos build`,
     `cargo xtask parity-check`;
   - `cargo xtask smoke`;
   - `scripts/e2e/tsduck-playback-gate.sh gate` if the pipeline changed;
   - green exact-SHA CI.

   Then mark the milestone `Complete` with evidence, and record the pin range
   with `cargo xtask progress add-session`.

---

## Provisional triage (confirmed per row; "Next→triage" = extract behavior per ADR 0094)

**ucu-01 → v26.4.0:** all 10 rows are upstream tooling (deps, CI, bundling,
changelog). Verify that `1e5c0b12` (rename to legacy) changes no config-folder or
data paths.

**ucu-02 → v26.5.0**
- Port: `ddc8ef7f` add-playout dropdown · `6ea0a296` XMLTV playout offset ·
  `7e38297d` classic fallback filler · `cd298820` skip already-extracted subtitles
- Next→triage (subtitle and audio priority):
  - `e6496bbc` audio stream selection
  - `57f7dfa5` image subtitle selection
  - `1b5d6af7` text subtitle conversion (has a migration)
  - `b1ce607c` media-server subtitles
  - `e04a834e`, `91b029e4` watermarks
  - `7869f165` scaling behavior
  - `bb493670` media server items
- N/A 0094: `ea4d3b65` streaming-engine setting · `2ed432f9` · `4ee97d5b` ·
  `f2706f32` · `9882b008` · `d8955134`
- Tooling: `f7853e80`, `5cef7775`, `290f991b`

**ucu-03 → v26.5.1:** Port `a1036430` orphan actor/artwork cleanup. Tooling:
`54779e07`, `637e6be8`.

**ucu-04 → v26.6.0**
- Port:
  - `0fc7ebb0` JWT in HLS direct playlist
  - `fecce92e` configured ffmpeg for graphics
  - `aa639375` case-insensitive mpeg-ts script
  - `7032cfc8` local subtitles (migration) · `32e54828` `und` language tag
  - `0b8c5146` keep serving old HLS segments
  - `2b7531ca` remove already-played playout items
  - `42937ac3` YAML scheduled playouts
  - `075649ad` Plex subtitle titles
- Adapt:
  - `dca93717`, `dbdf3dff` graphics (ADR 0005)
  - `620bc0bc` search index (ADR 0009)
  - `3e1918c9` release notes on home page
- Next→triage: `64296597` · `7c4a5e3a` · `cc9880c2` · `577983af` · `b1c7702a` ·
  `14ee8063` · `294a2ff1` dynamic fallback filler · `387da2b8` probe hints ·
  `0036fc5c`
- N/A 0094: `9c5d86a1`
- Tooling: the dependency, openapi and changelog rows

**ucu-05 → v26.7.0**
- Port:
  - `18137832` Plex titles
  - random start-point cluster `e5a2a763`/`0d692e70`/`4fb42f69` (migration)
  - `16e341d9` singleton mutex (platform check)
  - `f7bdcede`, `6ff44cf5`, `16b71569`, `a2f54ef1`, `ecf6c2c1`, `a7ecc5f9`
    scheduling fixes
  - `3d24c3f0` playout gap UI
  - `15db3c4c` URL logo watermarks
- Adapt:
  - `7850ced6` green-line workaround removal: remove
    `crates/ersatzrs-ffmpeg/src/pipeline/mod.rs:1566` only if the fix is proven
    in our FFmpeg 9 build; otherwise record `Covered`/deviation
  - `9cc771be`, `b5fd035b`: FFmpeg-version half only
- Next→triage: `4ffd775a` text subtitle fixes
- N/A 0063: `5855deb8`, `57a0c40d`
- Tooling: the rest

**ucu-06 → v26.8.0**
- Port: `5ab81525` mpeg-ts multi-byte characters · `f737de89` Jellyfin external
  subtitles (migration)
- Adapt: `33c33587` Vulkan in Docker
- Next→triage: `3a02c39b` multiple watermarks · `cd30ac4a` image graphics ·
  `b36e94cc` on-demand progress
- N/A 0094: `ad9848cc`, `9088ae1a`, `7b43e2f7`, `7c96203b`
- N/A 0063 and tooling: the rest

**ucu-07 → v26.8.1 (security window)**
- Port:
  - `14372575` management UI reachable over the streaming port
  - `a5937386` Plex sign-in (migration)
  - `c27a8d50` require API key
  - `ef5094aa` GHSA-h3r4-r2f2-qf59: signed internal URLs, subtitle and
    troubleshooting path validation
- Next→triage: `cefd715c` mirror channels

**ucu-08 → v26.9.0**
- Port:
  - `b7aaea3d` block scheduler deleting items
  - `722f1b93` YAML shuffle order
  - `06ee45ff` text subtitle playback
  - `34b0d954` playlist state in sequential and scripted schedules
  - `b0f313ee` playlist and marathon fixes (migration)
  - `b3a35116` artwork cleanup · `19b783d7` artwork authorization
  - `fd2171ad` fallback duration, history and guide group
  - `463a4c80` HDHR disabled when JWT is used
  - `9067cde8` tag maintenance (migration)
  - `1681d1ad`/`d5f11315` channel startup · `6ebcaf80` multiday seek
  - `4a2aa157` local folder etags · `188274e8` season scanning
- Tooling: `39e39ef6`, `fcd593e0`, `2cf8e56a`, `165c39ec`, `5bb3ddfc`

**ucu-09 → tip**
- Port:
  - `3c41959f` health checks · `96cef081` heal artwork cache
  - `7e141a13` static hash functions · `4c3683ae` logging and validation TODOs
  - `b3f7f3fe` YAML post-roll state · `c673082e` converter subtitles
  - `4fffa955` Plex other-video tags (migration)
  - `0840e0fb` troubleshoot music video credits (ADR 0076)
- Adapt: `28b2e00c` graphics canvas (ADR 0005) · `6e9693e9` QSV hardware padding
- Likely Covered: `cbfea01c` DV/HDR10 probe (ADR 0064, verify the persisted
  fields)
- Next→triage: `2989faeb` · `599dc5ea`
- Tooling: the changelog and trigger rows

**Upstream schema commits** get new SQLite migrations only (ADR 0004 and
ADR 0080): `1b5d6af7`, `7032cfc8`, `0d692e70`, `f737de89`, `a5937386`,
`b0f313ee`, `9067cde8`, `cbfea01c`, `4fffa955`. `ea4d3b65` is N/A under ADR 0094.

---

## Critical files

| File | Role |
|---|---|
| `docs/automation/parity-baseline.md` | Pin; advanced each step |
| `plans/upstream-catch-up.md` | New live plan and ledger |
| `docs/adr/0094-native-pipeline-instead-of-ersatztv-next-engine.md` | New ADR |
| `docs/progress/parity/upstream-baseline-catch-up.toml` | New tracking record |
| `docs/progress/milestones.toml` | `ucu-00..09`; `p9a-01` dependency |
| `docs/progress/phase.toml` | Focus and next steps |
| `CHANGELOG.md` | `[Unreleased]` entries per ported row |
| `migrations/` | New sqlx migrations for schema rows |
| Owning `docs/progress/parity/<slug>.toml` | For example `classic-scheduling`, `yaml-scheduling`, `scanner`, `ffmpeg-filter-graph`, `drawvg-graphics-engine`, `ersatzrs-healthcheck`, `hdhomerun-http-endpoints` |

Reused tooling: `cargo xtask progress check|add-session`, `cargo xtask affected-checks`,
`cargo xtask smoke`, `scripts/e2e/tsduck-playback-gate.sh`, and the skills
`parity-slice`, `add-migration`, `new-adr`, `update-parity`, `changelog-entry`.

## Verification (program level)

- **Per step:** focused checks, plus Codex review, plus
  `cargo xtask progress check`. The pin invariant holds.
- **Per milestone:** full gate, smoke, TSDuck when the pipeline changed, and
  green exact-SHA CI; the milestone is marked `Complete` with evidence.
- **Program done** when all of these hold:
  - the ledger has 152 filled rows, matching
    `git rev-list --count 0d301df5..495e3799`;
  - the pin equals `495e3799` and the `$ERSATZTV_SRC` HEAD matches;
  - no row is `Deferred` without maintainer approval;
  - ADR 0094 is Accepted and its Implementation is set;
  - `cargo xtask progress gaps` shows no unrecorded post-pin debt;
  - `ucu-09` is Complete, and `p9a-01` becomes runnable.
- **Future upstream deltas** (legacy keeps receiving fixes): open a new UCU
  window with the same procedure; never advance the pin automatically.
