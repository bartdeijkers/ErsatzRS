# Upstream ErsatzTV catch-up

## Latest upstream target and state

- Initial pin: `0d301df5e8512fdfccf3a611188d27ab1690ee31` (row 0; no upstream commit processed).
- Latest fetched target (`origin/main`, 2026-09-23): `ee038ba460a237e277c2ed2fbb413bfac13795f8`
  — feat: next engine rotation (#3034).
- Ledger: **169 commits**, **148 pending**. The original plan covered 152 through
  `495e3799`; the maintainer expanded the target to the latest upstream commit
  on 2026-09-19, so the ten newer commits are also pending in this ledger. The 2026-09-22
  refresh adds six more commits; a later refresh adds Next engine rotation.
  Both retain the existing ledger as an exact prefix.
- Refresh the target from upstream at the start of every run and append newly
  observed commits. The target is the latest fetched tip for that run, not a
  permanent cutoff. A bootstrap still processes no upstream commit.
- Ledger order comes from
  `git -C "$ERSATZTV_SRC" log --reverse --abbrev=8 --format='%h|%s' <initial-pin>..<target>`.
- Processed rows: **21**. Empty verdict/owner/ErsatzRS-commit cells mean pending.
- [Pin](../docs/automation/parity-baseline.md) is authoritative, followed by
  this ledger, followed by upstream checkout HEAD. All must agree before work.
  The [parity record](../docs/progress/parity/upstream-baseline-catch-up.toml)
  mirrors the target and backlog; a mismatch there is also a disagreement.
- [Milestones](../docs/progress/milestones.toml) own acceptance and evidence.
  AMM-N-03 remains a prerequisite; setup does not waive native acceptance.
- [ADR 0095](../docs/adr/0095-native-pipeline-instead-of-ersatztv-next-engine.md)
  is Accepted for Next as an optional second engine alongside the native
  pipeline. The rejected native-only proposal cannot justify exclusions.

## One-commit procedure

The maintainer's 2026-09-20 decision makes this a Codex-only workflow:
Codex performs the substantive review directly; no Claude or replacement
review agents are required. Earlier review attempts remain historical evidence.
On 2026-09-22 the maintainer authorized the prepared first local commit and
subsequent individually reviewed local UCU steps. This satisfies step 8 local
commit authorization; push, CI dispatch and release tagging remain separate.
The maintainer amended the CI cadence on 2026-09-22: skip individual CI runs
for UCU-01 and UCU-02. Their local gates and ordered row reviews still apply;
continue to the next window once those are complete. Run the first consolidated
exact-SHA CI after UCU-03 smoke; that CI checkpoint is explicitly authorized.
This supersedes the separate-CI requirement for these two windows only.

The maintainer also authorizes pushing to remote `main` after completion of
each milestone from UCU-03 through UCU-09 inclusive (UCU-05 explicitly confirmed).
This is milestone-boundary push authorization, not permission to push each
intermediate row. The subsequent explicit goal also authorizes running and
finishing green exact-SHA CI at every boundary from UCU-03 onward. Complete the
local window gates, push its candidate to remote main, verify terminal green CI,
and only then claim the milestone complete. Release tagging/publication remains
a separate action; a push alone is not CI proof.

Before selecting or processing row 1, AMM-N-03 must be accepted and ucu-00
closed against its recorded criteria. A documentation checkpoint does not
satisfy that prerequisite or authorize a later implementation commit.

1. Print the pin, last processed ledger row (or initial pin at row 0), and
   `$ERSATZTV_SRC` HEAD. Stop on disagreement; do not repair by guessing.
2. Fetch `origin/main` without moving upstream HEAD. Verify the previous target
   is an ancestor of the fetched tip and the existing ledger remains an exact
   prefix of the generated log; stop on divergence or fetch failure. Append
   every new commit with empty verdict/owner/commit fields, refresh the target
   and backlog count in every record that states them — this plan, the pin
   document's Catch-up mode section, and the catch-up parity record's
   `etv_source` and summary — and update the open ucu-09 tail milestone. Other
   documents describe the bootstrap as of a stated date and must not repeat the
   live target or count. If that milestone is already Complete, reopen it with the
   new scope while preserving earlier evidence; do not claim current catch-up
   completion from older acceptance. Select exactly the first pending row. Read its stat, full diff, tests,
   CHANGELOG hunk and surrounding files with `git show <sha>:<path>`; leave
   upstream HEAD at the pin until landing.
3. Map behavior through [PORTING.md](../PORTING.md) and the owning parity
   record's `etv_source`/`rs_target`. Record in notes the problem solved,
   whether it exists in Rust (code/test evidence), the benefit of porting,
   and any conflict with an accepted ADR. Provisional triage is not evidence.
4. Choose exactly one verdict: `Port`, `Adapt (ADR nnnn)`, `Covered`,
   `N/A (ADR nnnn)`, `N/A (upstream tooling)`, or `Superseded by <sha>`.
   Supersession applies only to consecutive fixes inside the same window;
   implement the final shape at the cluster's last row. A new ADR, Deferred
   verdict or acceptance change requires a maintainer decision.
5. For Port/Adapt use [parity-slice](../skills/parity-slice.md), port upstream
   tests at the narrowest level, snapshot generated FFmpeg/YAML output, use
   sqlx SQLite migrations, preserve subtitle/audio handling, update health,
   localize UI in every culture, credit the upstream PR in the changelog and
   update the owning parity record. Accepted ErsatzRS ADRs take precedence.
6. Use `CARGO_BUILD_JOBS=8` and serial builds: `cargo xtask affected-checks`,
   focused `./run.sh test -p <crate> <filter>` and scoped Clippy. Add smoke for
   playback changes, relevant Playwright and hydration lint for browser changes,
   and `cargo xtask progress check`. Do not run a full gate for an ordinary row.
7. Codex reviews the full diff and affected behavior directly. Record concrete
   findings with failure path, impact and smallest correction; fix actionable
   serious defects and review the corrections. Record remaining limitations
   without claiming unproven acceptance.
8. Only with maintainer commit authorization, land the change, filled row and
   updated pin/subject/URL together; use Conventional Commits and body
   `Upstream: ErsatzTV <sha> (#PR)` (omit PR when absent). To avoid a self-hash,
   the landing row may identify its ErsatzRS commit by unique commit subject;
   resolve it with `git log --grep` when reading the ledger.
   Then `git -C "$ERSATZTV_SRC" checkout <sha>`. Stop after one row; no batching
   during this task, no push, CI dispatch or release tagging.
9. At a milestone boundary, run its acceptance gates with the CI cadence
   amendment above: UCU-01/UCU-02 close locally; the first CI follows UCU-03
   smoke. Push and exact-SHA CI are authorized for UCU-03 through UCU-09.
   After local window acceptance, push the candidate, run/monitor CI and resolve
   failures until that exact candidate is green before marking Complete. Record
   evidence and a progress session; never claim skipped or queued CI as proof.
10. Report upstream SHA/subject, verdict and rationale, changed files, evidence,
    old/new pin, row count, next commit and remaining blocker.

## Row 20 source map: media-server inputs

Upstream `bb493670` changes `Application/Playouts/Commands/SyncNextPlayoutHandler.cs`
and `Core/Next/Playout.cs`; it has no UI, migration, test or changelog changes.
The handler exports media-server items, prefers an existing local file after
provider-specific path replacement, otherwise uses internal HTTP routes for
Plex files and Jellyfin/Emby movies or episodes. Missing audio produces a lavfi
silence track. Wire-model renames consolidate shared and per-track sources.

- Upstream services: `Core/Extensions/MediaItemExtensions.cs::GetLocalPath` and
  `Core/{Plex,Jellyfin,Emby}/*PathReplacementService.cs` select replacements for
  the item's library/source. `Controllers/InternalController.cs` resolves the
  internal media URLs through provider connection queries and redirects to the
  server. Plex uses the selected media file's key, not its filesystem path.
- ErsatzRS persistence: `next_engine/schedule.rs` selects one ordered version
  and file, respecting exact version pins. Retain the selected file ID with its
  version/path so HTTP resolution cannot select a different Plex file by path.
  `repositories/media_source.rs` already exposes per-source replacements and
  connection records. Provider item/file tables retain remote identities.
- ErsatzRS transport: `next_engine/protocol.rs::Source::Http` already models
  URI, headers, timeout, reconnect and seek options. `selection.rs` preserves
  complete sources for selected tracks and inserts finite silence without
  reconstructing the video source. The upstream no-audio branch copies only
  source type/path; do not lose the HTTP URI when applying that behavior.
- Integration: `next_engine/source.rs` resolves provider-scoped local paths;
  remote publication uses a process-capability-protected route in the existing
  web host. `next_engine/media.rs` validates the exact item/version/file binding,
  loads current provider credentials at media-open time, and streams HTTP through
  the web route with byte-range status and headers retained. Published Next JSON
  contains the internal capability rather than provider credentials. Local
  libraries retain the native replacement list. Scoped credentials take precedence;
  legacy global fallback is restricted to one configured server of that kind.
  `next_engine/connections.rs` supplies bounded identity-verified Plex selection
  and transactional activation outside schedule build locks. The media-open
  integration has type-check, focused credential/range/binding and web response
  proof, decoded Next playback including Plex failover, bounded redirect review
  and real-host HTTP/preview smoke. UCU-02 owns the row acceptance evidence.
- Side effects and evidence: keep complete atomic schedule publication,
  mirror timing, exact stream pins, redacted errors, session ownership and
  cancellation. Test overlapping paths across servers, selected file/version
  identity, local preference, HTTP fallback for all three providers, missing
  audio, seek and retained subtitle/audio sources, then decoded Next playback.
  Remote Stream/add-on lifetimes and later graphics rows are separate scope.
- Intentional deviations: the optional engine and atomic publication remain
  governed by ADR 0095. Row 20 is Port; no new deviation or Next exemption is
  introduced. Full-window and platform acceptance remain owned by UCU-02.

## Row 21 source map: audio selection during Next publication

Upstream `e6496bbc` (#2866) changes six files: the Next sync handler, default
stream-selector interface/implementation, native caller, selector tests and
scanner fake. It calls custom YAML selection before the default selector while
building Next items, retaining existing indexes and item/channel preferences.
The new logging flag suppresses routine selection diagnostics during publication.
There is no schema, UI, changelog or FFmpeg argument-builder change.

The default selector also invokes movie/episode JavaScript callbacks. Existing
Rust YAML/language selection alone therefore cannot cover this row. The source
map includes `FFmpegStreamSelector`, `ScriptEngine`, `StreamSelectorRepository`
and both upstream resource scripts. The new `infra/audio_script.rs` adapter
preserves callback metadata and uses the accepted ADR 0096 runtime constraints.
Native HLS/MPEG-TS, troubleshooting and Next call it only after stronger audio
choices, with ordinary selection as fallback. Native transport remaps selected
indexes by identity when an interval source changes its stream layout.
Next's existing explicit track source, seek and subtitle projection remain intact.

The row is an adaptation under ADR 0096; unsupported CLR interop and the
statement-counter replacement are accepted decisions. UCU-02 owns local proof;
full native execution on all six platforms remains an explicit program gate.

## Row 22 source map: Classic fallback intervals

Upstream `7e38297d` (#2867) changes `PlayoutModeSchedulerBase` and five scheduler
test files. Gap fallback and padding fallback must emit as many real media
intervals as needed, clamp the last interval, advance the filler enumerator for
each interval and preserve chapter offsets, parent identity and chapter title.
One, Multiple, Duration and Flood share this behavior. No UI, schema, FFmpeg
builder, changelog or source-transport changes occur upstream.

Rust targets are `core/scheduling/classic` (gap/pad generation and continuation)
and the scheduling repository (chapter-as-media filler hydration). Existing
Rust gap fallback stretches one item and clears its out-point; padding omits
fallback, and filler hydration ignores the persisted chapter option. These are
required parts of the row, not covered behavior. Tests must prove exact rows,
clipping, watermark/guide metadata, chapter boundaries and continued filler order.
Existing Refresh/Reset safety and audio/subtitle identity remain authoritative.
The first checkpoint implements real clip intervals for gaps and post-roll pad,
SQL chapter-as-media hydration, source offsets and serialized filler positions.
The Duration overflow path must retain the real cursor so tail fallback sees
its unfilled interval. A second checkpoint supplies source chapter boundaries,
places mid-roll before the following chapter and post-roll after the final one,
and rounds padding on wall-clock boundaries. `FillerExpression` semantics and
remaining persistence/runtime proof still prevent row landing. See the owning
classic-scheduling debt and UCU-02 evidence.

## Provisional triage and ledger

Seeds below come from [the original plan](ErsatzRS_catchup.md#provisional-triage-confirmed-per-row-nexttriage--extract-behavior-per-adr-0094).
The maintainer selected Next as an optional second engine on 2026-09-22.
Next-specific exclusions from the rejected native-only proposal are withdrawn;
review integration as required behavior under Accepted ADR 0095.
Its Next-engine references to 0094 mean the newly allocated **0095** here;
0094 already owns automatic add-on publication. Notes are hypotheses only.
Unlisted rows retain the plan's tooling hypothesis, or FFmpeg/tooling hypothesis
in ucu-06. The ten newly appended rows have no inherited triage. Every full diff still needs review, including mixed behavior/tooling.
For row 1 verify the rename does not change config-folder or data paths; for
`7850ced6` verify the green-line fix in our FFmpeg 9 build before removing it.
Schema rows and the random-start cluster retain the original plan's cautions.

| # | upstream sha | subject (PR) | verdict | owning parity slug | ErsatzRS commit | notes |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `1e5c0b12` | rename to ersatztv legacy (#2842) | Covered | support-pages | `docs(porting): reconcile upstream legacy rename and accept optional Next` | ucu-01; release-notes removal already present; upstream-only branding/tooling needs no Rust edit. See ucu-01 evidence. |
| 2 | `848db7ea` | even more dependency updates (#2848) | Port | hdhomerun-http-endpoints | `fix(web): correct HDHomeRun manufacturer repository link` | ucu-01; correct native manufacturer repository URL; remaining changes are .NET API/dependency and upstream branding maintenance. See ucu-01 evidence. |
| 3 | `5d779635` | update last dependencies (#2849) | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream dependency maintenance` | ucu-01; only YamlDotNet/Refit package-version attributes change; native Cargo dependencies do not consume them. See ucu-01 evidence. |
| 4 | `47885758` | bundle ersatztv next artifacts (#2850) | Port | upstream-baseline-catch-up | `build(release): bundle optional Next engine artifacts` | ucu-01; verified artifact staging, license/provenance and Docker startup. Runtime integration remains open under ADR 0095. See ucu-01 evidence. |
| 5 | `1b292b5f` | try to fix github token | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream workflow token references` | ucu-01; token-only workflow correction; native uploads already use github.token and public Next staging needs no token. See ucu-01 evidence. |
| 6 | `f69761cb` | more gh token tweaks | Covered | upstream-baseline-catch-up | `docs(porting): reconcile upstream CLI artifact publication` | ucu-01; native asset upload already uses gh release; upstream develop cleanup and reusable-token wiring do not apply. See ucu-01 evidence. |
| 7 | `b76d8f8e` | more token alignment | Covered | upstream-baseline-catch-up | `docs(porting): reconcile upstream job-token alignment` | ucu-01; native uploads already bind github.token; Next downloads need no token and upstream develop cleanup does not apply. See ucu-01 evidence. |
| 8 | `30c361db` | only bundle the ersatztv-channel binary (#2851) | Port | upstream-baseline-catch-up | `build(release): bundle only the Next channel executable` | ucu-01; only channel binary beside server, with license/provenance retained. Packaging tests and Docker probe pass. See ucu-01 evidence. |
| 9 | `74a21abd` | fix workflow | Covered | upstream-baseline-catch-up | `docs(porting): reconcile upstream workflow syntax repair` | ucu-01; removes the stray workflow line already excluded during row 8. See ucu-01 evidence. |
| 10 | `a74896df` | prep for release v26.4.0 [no ci] | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream April release notes` | ucu-01; upstream changelog publication only. Window acceptance gates remain open; do not start UCU-02 yet. |
| 11 | `ea4d3b65` | add channel streaming engine setting (#2852) | Port | channels-admin-ui | `feat(web): persist optional Next engine selection` | ucu-02; additive migration, native default, engine roundtrip, Next HLS Segmenter normalization, localized SSR editor and ETV_NEXT_FOLDER boundary. Focused Rust/Clippy and Chromium/Firefox with/without JS pass. Actual Next execution remains subsequent integration work under ADR 0095. See ucu-02 evidence. |
| 12 | `ddc8ef7f` | fix: add playout button should open drop down menu (#2853) | Covered | playout-list-detail | `test(web): verify upstream menu activator coverage` | ucu-02; native playout toggle and direct show Add To/Quick/Deep Scan handlers already work. Four Chromium/Firefox regressions prove dropdown visibility, keyboard toggle, dialog activation and scan parameters. No Rust change. See ucu-02 evidence and media-browsers parity. |
| 13 | `2ed432f9` | feat: enable ersatztv next streaming engine (#2854) | Adapt (ADR 0095) | upstream-baseline-catch-up | `feat(infra): complete initial Next engine integration` | ucu-02; supervised local-video HLS, preview, track selection, atomic publication and Continue refresh verified. Provider/graphics rows and window/platform acceptance remain open. See ucu-02 evidence. |
| 14 | `4ee97d5b` | fix: fix next playout building (#2855) | Covered | upstream-baseline-catch-up | `test(web): verify Next channel update propagation` | ucu-02; existing mirror-offset projection, post-save publication notifications and executable-relative runtime discovery cover all three upstream corrections. See ucu-02 review and focused tests. |
| 15 | `f7853e80` | bundle ersatztv-channel binary with macos app (#2856) | Covered | upstream-baseline-catch-up | `docs(release): reconcile Next macOS bundling` | ucu-02; both macOS RID mappings and executable-adjacent archive staging already exist; six offline tests and actual relocation/archive snippets pass. Active macOS release/runtime acceptance remains explicit. See ucu-02 evidence. |
| 16 | `5cef7775` | fix duplicate windows zips on develop release | Covered | upstream-baseline-catch-up | `docs(release): reconcile Windows upload shell selection` | ucu-02; native upload already explicitly selects Bash with clobber; beta uses explicit PowerShell; no moving develop-tag cleanup. Workflow lint passes with declared self-hosted labels. See ucu-02 evidence. |
| 17 | `6ea0a296` | fix: consistently apply playout offset when generating xmltv (#2863) | Port | xmltv-guide | `fix(infra): preserve signed mirror offsets on channel upsert` | ucu-02; retain existing four-timestamp projection and fix negative-offset persistence; actual/guide windows, both signs and unchanged source items verified. See ucu-02 evidence. |
| 18 | `f2706f32` | feat: support playout json on windows (#2864) | Adapt (ADR 0095) | upstream-baseline-catch-up | `test(infra): verify Next publication on Windows` | ucu-02; accepted atomic-file publication passes seven native Windows/NTFS tests, including sharing-denial recovery, and six Linux tests. Full Windows Next playback remains open. See ucu-02 evidence. |
| 19 | `9882b008` | tweak streaming engine editor | Covered | channels-admin-ui | `docs(web): reconcile Next editor mode normalization` | ucu-02; existing editor immediately selects HLS Segmenter for Next and retains server-side no-JS normalization. Unchanged row-11 Chromium/Firefox evidence covers the setter behavior. See ucu-02 evidence. |
| 20 | `bb493670` | feat: support media server items in next streaming engine  (#2865) | Port | upstream-baseline-catch-up | `feat(infra): resolve Next media at playback time` | ucu-02; provider-scoped paths, exact file binding, current credentials, bounded Plex failover and Range-preserving media opens. Decoded three-provider/failover proof and Next HTTP/preview smoke pass. See ucu-02 evidence. |
| 21 | `e6496bbc` | feat: select audio streams for next engine during playout build (#2866) | Adapt (ADR 0096) | upstream-baseline-catch-up | `feat(infra): embed bounded JavaScript audio selectors` | ucu-02; shared native/Next callbacks, metadata and YAML/pin precedence; bounded execution and fallback tests, decoded alternate audio, native/Next smoke. Full platform acceptance remains open. See ucu-02 evidence. |
| 22 | `7e38297d` | fix: schedule all required fallback filler in classic schedules (#2867) | | | | ucu-02; provisional: Port candidate. |
| 23 | `57f7dfa5` | feat: select embedded image subtitle streams for next engine during playout build (#2868) | | | | ucu-02; provisional: Next behavior triage. |
| 24 | `1b5d6af7` | feat: convert text subtitles using next engine (#2869) | | | | ucu-02; provisional: Next behavior triage. |
| 25 | `cd298820` | fix: don't extract embedded subtitles that have already been extracted (#2873) | | | | ucu-02; provisional: Port candidate. |
| 26 | `e04a834e` | feat: overlay single permanent watermarks using next engine (#2878) | | | | ucu-02; provisional: Next behavior triage. |
| 27 | `91b029e4` | feat: overlay single intermittent watermarks using next engine (#2879) | | | | ucu-02; provisional: Next behavior triage. |
| 28 | `7869f165` | feat: use configured scaling behavior with next engine (#2880) | | | | ucu-02; provisional: Next behavior triage. |
| 29 | `b1ce607c` | feat: convert subtitles from media servers (#2882) | | | | ucu-02; provisional: Next behavior triage. |
| 30 | `d8955134` | feat: use amf and rkmpp hw accel with next engine (#2883) | | | | ucu-02; provisional: Next integration candidate under Accepted ADR 0095. |
| 31 | `290f991b` | prep for release v26.5.0 [no ci] | | | | ucu-02; provisional: Upstream tooling candidate. |
| 32 | `a1036430` | fix: clean up orphan actor and artwork records (#2885) | | | | ucu-03; provisional: Port candidate. |
| 33 | `54779e07` | fix: pin nvencsharp lib to 2.0.0 for linux compatibility (#2887) | | | | ucu-03; provisional: Upstream tooling candidate. |
| 34 | `637e6be8` | prep for release v26.5.1 [no ci] | | | | ucu-03; provisional: Upstream tooling candidate. |
| 35 | `0fc7ebb0` | fix: use jwt in hls direct playlist (#2889) | | | | ucu-04; provisional: Port candidate. |
| 36 | `64296597` | feat: add channel config overlays for next engine (#2891) | | | | ucu-04; provisional: Next behavior triage. |
| 37 | `fecce92e` | fix: use configured ffmpeg with all graphics elements (#2892) | | | | ucu-04; provisional: Port candidate. |
| 38 | `41bf22eb` | chore: dependency updates (#2893) | | | | ucu-04; provisional: Upstream tooling candidate. |
| 39 | `7c4a5e3a` | feat: support remote streams with next engine (#2899) | | | | ucu-04; provisional: Next behavior triage. |
| 40 | `2c02561f` | chore: update dependencies (#2900) | | | | ucu-04; provisional: Upstream tooling candidate. |
| 41 | `cc9880c2` | feat: always save error reports with next engine (#2907) | | | | ucu-04; provisional: Next behavior triage. |
| 42 | `822c7547` | pin imagesharp dependency (#2909) | | | | ucu-04; provisional: Upstream tooling candidate. |
| 43 | `9c5d86a1` | feat: show next version in ui (#2910) | | | | ucu-04; provisional: Next integration candidate under Accepted ADR 0095. |
| 44 | `577983af` | feat: support explicitly scheduled images using next engine (#2911) | | | | ucu-04; provisional: Next behavior triage. |
| 45 | `b1c7702a` | fix: properly sync in and out points with next engine (#2912) | | | | ucu-04; provisional: Next behavior triage. |
| 46 | `99a1ecee` | chore: update dependencies (#2915) | | | | ucu-04; provisional: Upstream tooling candidate. |
| 47 | `aa639375` | fix: use case-insensitive lookup for mpeg-ts script (#2916) | | | | ucu-04; provisional: Port candidate. |
| 48 | `14ee8063` | feat: sync generated music video credits to next engine (#2917) | | | | ucu-04; provisional: Next behavior triage. |
| 49 | `294a2ff1` | feat: support dynamic fallback filler when using next engine (#2919) | | | | ucu-04; provisional: Next behavior triage. |
| 50 | `387da2b8` | feat: use probe hints with next engine (#2920) | | | | ucu-04; provisional: Next behavior triage. |
| 51 | `679fb172` | chore: update refit to work around cert revocation (#2921) | | | | ucu-04; provisional: Upstream tooling candidate. |
| 52 | `dca93717` | fix: improve speed of motion graphics element compositing (#2922) | | | | ucu-04; provisional: Adapt candidate. |
| 53 | `dbdf3dff` | feat: add epg_entries support to image graphics elements (#2923) | | | | ucu-04; provisional: Adapt candidate. |
| 54 | `7032cfc8` | fix: better handling of local subtitles (#2924) | | | | ucu-04; provisional: Port candidate. |
| 55 | `32e54828` | fix: use und language tag with local subtitles (#2925) | | | | ucu-04; provisional: Port candidate. |
| 56 | `6b74da3c` | chore: update docker image (#2926) | | | | ucu-04; provisional: Upstream tooling candidate. |
| 57 | `3756e7fb` | chore: update dependencies (#2927) | | | | ucu-04; provisional: Upstream tooling candidate. |
| 58 | `0b8c5146` | fix: continue serving old segments when unable to transcode in realtime (#2933) | | | | ucu-04; provisional: Port candidate. |
| 59 | `2b7531ca` | fix: remove already-played playout items (#2934) | | | | ucu-04; provisional: Port candidate. |
| 60 | `c8bd9766` | chore: update openapi to fix build (#2936) | | | | ucu-04; provisional: Upstream tooling candidate. |
| 61 | `42937ac3` | feat: YAML Scheduled Playouts (#2935) | | | | ucu-04; provisional: Port candidate. |
| 62 | `0036fc5c` | fix: subtitles with next engine (#2938) | | | | ucu-04; provisional: Next behavior triage. |
| 63 | `620bc0bc` | fix: properly delete manual collections from search index (#2939) | | | | ucu-04; provisional: Adapt candidate. |
| 64 | `075649ad` | fix: synchronize subtitle titles from plex libraries (#2940) | | | | ucu-04; provisional: Port candidate. |
| 65 | `3e1918c9` | feat: restore release notes on home page (#2942) | | | | ucu-04; provisional: Adapt candidate. |
| 66 | `95fa937c` | chore: update skia sharp (#2943) | | | | ucu-04; provisional: Upstream tooling candidate. |
| 67 | `0b44ad47` | prep for release v26.6.0 [no ci] | | | | ucu-04; provisional: Upstream tooling candidate. |
| 68 | `18137832` | fix: update plex movie and other video titles (#2947) | | | | ucu-05; provisional: Port candidate. |
| 69 | `e5a2a763` | fix: randomize start points with classic playlists (#2948) | | | | ucu-05; provisional: Port candidate. |
| 70 | `0d692e70` | fix: always randomize start points (#2949) | | | | ucu-05; provisional: Port candidate. |
| 71 | `4fb42f69` | fix: random start point is needed with every playlist build (#2950) | | | | ucu-05; provisional: Port candidate. |
| 72 | `16e341d9` | fix: key singleton mutex on config folder (#2946) | | | | ucu-05; provisional: Port candidate. |
| 73 | `5855deb8` | feat: upgrade ffmpeg to 8.1.2 (#2951) | | | | ucu-05; provisional: N/A candidate under ADR 0063. |
| 74 | `6d8666b8` | chore: update dependencies (#2952) | | | | ucu-05; provisional: Upstream tooling candidate. |
| 75 | `7850ced6` | fix: remove green line workarounds; etv's ffmpeg 8.1.2 fixes (#2953) | | | | ucu-05; provisional: Adapt candidate. |
| 76 | `f7bdcede` | fix: checkpoint classic schedules every day (#2954) | | | | ucu-05; provisional: Port candidate. |
| 77 | `6ff44cf5` | fix: don't prematurely reset shuffled collection progress (#2955) | | | | ucu-05; provisional: Port candidate. |
| 78 | `16b71569` | fix: playlist enumerator consistency fixes (#2956) | | | | ucu-05; provisional: Port candidate. |
| 79 | `a2f54ef1` | fix: block playout build loop (#2957) | | | | ucu-05; provisional: Port candidate. |
| 80 | `9cc771be` | feat: show next and ffmpeg version in troubleshooting (#2958) | | | | ucu-05; provisional: Adapt candidate. |
| 81 | `3f2efcd4` | fix: discover tests in ErsatzTV.Tests (#2959) | | | | ucu-05; provisional: Upstream tooling candidate. |
| 82 | `4ffd775a` | fix: text subtitle fixes (#2960) | | | | ucu-05; provisional: Next behavior triage. |
| 83 | `05dd6cbe` | prep for release v26.7.0 [no ci] | | | | ucu-05; provisional: Upstream tooling candidate. |
| 84 | `15db3c4c` | fix: don't ignore channel logo watermarks that are urls (#2961) | | | | ucu-05; provisional: Port candidate. |
| 85 | `ecf6c2c1` | fix: classic playout playlist progress (#2966) | | | | ucu-05; provisional: Port candidate. |
| 86 | `a7ecc5f9` | fix: improve flexible start time handling near midnight (#2967) | | | | ucu-05; provisional: Port candidate. |
| 87 | `3d24c3f0` | fix: playout gap correctness in ui (#2968) | | | | ucu-05; provisional: Port candidate. |
| 88 | `57a0c40d` | fix: bundle newly patched ffmpeg for qsv on windows (#2969) | | | | ucu-05; provisional: N/A candidate under ADR 0063. |
| 89 | `b5fd035b` | feat: show next and ffmpeg versions in troubleshooting results (#2970) | | | | ucu-05; provisional: Adapt candidate. |
| 90 | `35b5a259` | prep for release v26.7.0 [no ci] | | | | ucu-05; provisional: Upstream tooling candidate. |
| 91 | `5ab81525` | fix: mpeg-ts playback with multi-byte characters (#2971) | | | | ucu-06; provisional: Port candidate. |
| 92 | `f737de89` | fix: jellyfin external subtitles (#2972) | | | | ucu-06; provisional: Port candidate. |
| 93 | `ad9848cc` | feat: troubleshoot next engine playback (#2973) | | | | ucu-06; provisional: Next integration candidate under Accepted ADR 0095. |
| 94 | `9088ae1a` | feat: more improvements to next troubleshooting (#2974) | | | | ucu-06; provisional: Next integration candidate under Accepted ADR 0095. |
| 95 | `7b43e2f7` | chore: update next channel config model (#2975) | | | | ucu-06; provisional: Next integration candidate under Accepted ADR 0095. |
| 96 | `3b53b69e` | chore: update dependencies (#2976) | | | | ucu-06; provisional: FFmpeg/tooling candidate. |
| 97 | `aae2529f` | chore: update dependencies (#2977) | | | | ucu-06; provisional: FFmpeg/tooling candidate. |
| 98 | `7c96203b` | chore: update playout model to get new next version (#2978) | | | | ucu-06; provisional: Next integration candidate under Accepted ADR 0095. |
| 99 | `9f83c8d2` | fix: new ffmpeg for windows vulkan/cuda interop (#2979) | | | | ucu-06; provisional: FFmpeg/tooling candidate. |
| 100 | `24eab260` | ci: remove dependabot [no ci] | | | | ucu-06; provisional: FFmpeg/tooling candidate. |
| 101 | `11789e8d` | chore: update dependencies (#2980) | | | | ucu-06; provisional: FFmpeg/tooling candidate. |
| 102 | `33c33587` | feat: enable vulkan in docker (#2981) | | | | ucu-06; provisional: Adapt candidate. |
| 103 | `5febb943` | docs: update changelog | | | | ucu-06; provisional: FFmpeg/tooling candidate. |
| 104 | `080039be` | chore: update windows ffmpeg (#2982) | | | | ucu-06; provisional: FFmpeg/tooling candidate. |
| 105 | `3a02c39b` | feat: support multiple watermarks using next engine (#2983) | | | | ucu-06; provisional: Next behavior triage. |
| 106 | `cd30ac4a` | feat: image graphics element support in next (#2984) | | | | ucu-06; provisional: Next behavior triage. |
| 107 | `b36e94cc` | fix: on-demand channel progress using next engine (#2985) | | | | ucu-06; provisional: Next behavior triage. |
| 108 | `c0e4c4ac` | chore: update windows ffmpeg (#2988) | | | | ucu-06; provisional: FFmpeg/tooling candidate. |
| 109 | `7b7a19a2` | prep for release v26.8.0 [no ci] | | | | ucu-06; provisional: FFmpeg/tooling candidate. |
| 110 | `14372575` | fix: close hole that allowed management ui over streaming port (#2989) | | | | ucu-07; provisional: Port candidate. |
| 111 | `a5937386` | fix: plex sign in (#2990) | | | | ucu-07; provisional: Port candidate. |
| 112 | `c27a8d50` | feat: require api key (#2991) | | | | ucu-07; provisional: Port candidate. |
| 113 | `cefd715c` | fix: mirror channels falling out of sync when using the next streaming engine | | | | ucu-07; provisional: Next behavior triage. |
| 114 | `ef5094aa` | fix: GHSA-h3r4-r2f2-qf59 | | | | ucu-07; provisional: Port candidate. |
| 115 | `49b68314` | prep for release v26.8.1 [no ci] | | | | ucu-07; provisional: Upstream tooling candidate. |
| 116 | `39e39ef6` | changelog update [no ci] | | | | ucu-08; provisional: Upstream tooling candidate. |
| 117 | `b7aaea3d` | fix: block scheduler deleting playout items (#2992) | | | | ucu-08; provisional: Port candidate. |
| 118 | `fcd593e0` | chore: trigger new build for updated next engine | | | | ucu-08; provisional: Upstream tooling candidate. |
| 119 | `2cf8e56a` | update changelog [no ci] | | | | ucu-08; provisional: Upstream tooling candidate. |
| 120 | `722f1b93` | fix: preserve YAML shuffle order across builds (#2993) | | | | ucu-08; provisional: Port candidate. |
| 121 | `06ee45ff` | fix: text subtitle playback (#2994) | | | | ucu-08; provisional: Port candidate. |
| 122 | `34b0d954` | fix: playlist state in sequential and scripted schedules (#2995) | | | | ucu-08; provisional: Port candidate. |
| 123 | `b0f313ee` | fix: multiple playlist and marathon fixes (#2996) | | | | ucu-08; provisional: Port candidate. |
| 124 | `b3a35116` | fix: artwork cleanup (#2997) | | | | ucu-08; provisional: Port candidate. |
| 125 | `19b783d7` | fix: authorize artwork when requested from ui (#2998) | | | | ucu-08; provisional: Port candidate. |
| 126 | `fd2171ad` | fix: fallback duration, history and guide group for sequential and scripted schedules | | | | ucu-08; provisional: Port candidate. |
| 127 | `463a4c80` | fix: disable hdhr controller when jwt is used (#2999) | | | | ucu-08; provisional: Port candidate. |
| 128 | `165c39ec` | fix: give slow test more time (#3000) | | | | ucu-08; provisional: Upstream tooling candidate. |
| 129 | `9067cde8` | fix: tag maintenance (#3001) | | | | ucu-08; provisional: Port candidate. |
| 130 | `1681d1ad` | fix: channel startup (#3002) | | | | ucu-08; provisional: Port candidate. |
| 131 | `6ebcaf80` | fix: multiday seek | | | | ucu-08; provisional: Port candidate. |
| 132 | `d5f11315` | fix: more channel startup fixes (#3003) | | | | ucu-08; provisional: Port candidate. |
| 133 | `4a2aa157` | fix: local folder etags (#3004) | | | | ucu-08; provisional: Port candidate. |
| 134 | `188274e8` | fix: season scanning fixes (#3005) | | | | ucu-08; provisional: Port candidate. |
| 135 | `5bb3ddfc` | prep for release v26.9.0 [no ci] | | | | ucu-08; provisional: Upstream tooling candidate. |
| 136 | `3c41959f` | fix: health check improvements (#3006) | | | | ucu-09; provisional: Port candidate. |
| 137 | `96cef081` | fix: heal artwork cache (#3007) | | | | ucu-09; provisional: Port candidate. |
| 138 | `7e141a13` | fix: use static hash functions (#3008) | | | | ucu-09; provisional: Port candidate. |
| 139 | `2989faeb` | fix: sessions api with next engine (#3009) | | | | ucu-09; provisional: Next behavior triage. |
| 140 | `6e9693e9` | feat: add hw accelerated padding for qsv (#3010) | | | | ucu-09; provisional: Adapt candidate. |
| 141 | `8cc34a1e` | chore: update changelog (and get new next build, docker image) | | | | ucu-09; provisional: Upstream tooling candidate. |
| 142 | `00db622d` | chore: trigger new build for updated next engine | | | | ucu-09; provisional: Upstream tooling candidate. |
| 143 | `cbfea01c` | feat: probe dv and hdr10 metadata (#3011) | | | | ucu-09; provisional: Likely Covered; verify ADR 0064 persisted fields. |
| 144 | `b30d9670` | chore: update changelog for new next build | | | | ucu-09; provisional: Upstream tooling candidate. |
| 145 | `4c3683ae` | chore: address four TODOs asking for logging and validation (#2964) | | | | ucu-09; provisional: Port candidate. |
| 146 | `28b2e00c` | feat: graphics canvas (#3012) | | | | ucu-09; provisional: Adapt candidate. |
| 147 | `b3f7f3fe` | fix: preserve YAML post-roll state across playout builds (#3013) | | | | ucu-09; provisional: Port candidate. |
| 148 | `c673082e` | fix: playout item converter subtitles (#3014) | | | | ucu-09; provisional: Port candidate. |
| 149 | `4fffa955` | fix: plex other video tags (#3015) | | | | ucu-09; provisional: Port candidate. |
| 150 | `599dc5ea` | feat: show errors over next fallback by default (#3016) | | | | ucu-09; provisional: Next behavior triage. |
| 151 | `0840e0fb` | feat: troubleshoot music video credits (#3017) | | | | ucu-09; provisional: Port candidate. |
| 152 | `495e3799` | chore: update changelog for new next build | | | | ucu-09; provisional: Upstream tooling candidate. |
| 153 | `0e1b355f` | chore: update dependencies (#3018) | | | | ucu-09; untriaged addition from latest upstream refresh. |
| 154 | `908fa961` | fix: graphics engine motion (#3019) | | | | ucu-09; untriaged addition from latest upstream refresh. |
| 155 | `d1c2ab3a` | chore: updates for new next, ffmpeg builds | | | | ucu-09; untriaged addition from latest upstream refresh. |
| 156 | `a4642d95` | fix: api response codes (#3020) | | | | ucu-09; untriaged addition from latest upstream refresh. |
| 157 | `2bbfbd10` | fix: playback troubleshooting (#3021) | | | | ucu-09; untriaged addition from latest upstream refresh. |
| 158 | `f6bd089f` | fix: entity locker concurrency (#3022) | | | | ucu-09; untriaged addition from latest upstream refresh. |
| 159 | `94c8c014` | fix: scheduler service reliability (#3023) | | | | ucu-09; untriaged addition from latest upstream refresh. |
| 160 | `b403012f` | fix: movie custom order ui (#3024) | | | | ucu-09; untriaged addition from latest upstream refresh. |
| 161 | `810c1dbd` | fix: font cache thread safety (#3025) | | | | ucu-09; untriaged addition from latest upstream refresh. |
| 162 | `4e951c17` | fix: mpeg-ts script reliability (#3026) | | | | ucu-09; untriaged addition from latest upstream refresh. |
| 163 | `db5a81e4` | fix: scripted schedule file validation (#3027) | | | | ucu-09; untriaged addition from 2026-09-22 upstream refresh. |
| 164 | `3f4255be` | fix: many ui bug fixes (#3028) | | | | ucu-09; untriaged addition from 2026-09-22 upstream refresh. |
| 165 | `3054e5d0` | fix: more bug fixes (#3029) | | | | ucu-09; untriaged addition from 2026-09-22 upstream refresh. |
| 166 | `3d31bfdd` | fix: remote stream yaml extension (#3030) | | | | ucu-09; untriaged addition from 2026-09-22 upstream refresh. |
| 167 | `5eb9102c` | fix: local library path validation (#3031) | | | | ucu-09; untriaged addition from 2026-09-22 upstream refresh. |
| 168 | `55885508` | prep for release v26.10.0 [no ci] | | | | ucu-09; untriaged addition from 2026-09-22 upstream refresh. |
| 169 | `ee038ba4` | feat: next engine rotation (#3034) | | | | ucu-09; untriaged addition from 2026-09-22 upstream refresh. |
