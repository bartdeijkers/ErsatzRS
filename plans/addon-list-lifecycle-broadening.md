# Remaining add-on lifecycle work

Milestones `amm-n-02` and `amm-n-03` in
[the tracker](../docs/progress/milestones.toml) own acceptance, completed results
and blockers. J-M and N-01 are implemented; their history lives in those records.
The withdrawn O-01 folder rebuild has no future scope. This plan does not
reintroduce withdrawn updater administration or retired provider acceptance.

Complete the [locked-design adaptation](amm-n-01b-addon-list-design-adaptation.md)
and its [workspace testing](addon-list-workspace-ux-refinements.md) under N-01B.
Use [automatic publication](automatic-addon-publication.md) and ADR 0094 for
current saved-change behavior, including when testing earlier lifecycle features.

### AMM-N-02 — Warn about a failed update when yt-dlp is behind the latest release

The maintainer narrowed this slice on 2026-09-11. Milestone `amm-n-02` owns
acceptance; ADR 0085 owns the decision. Implement only the official yt-dlp
add-on health warning, without expanding host update administration.

1. Map latest-release evidence from native yt-dlp and existing update outcomes
   into the provider health path. Any confirmed newer release for the configured
   channel is stale; an unknown latest version is not, and age is not a proxy.
2. Report a localized, redacted warning only when the tool could not update
   and remains stale. Reuse existing health/status presentation and include
   provider-supplied program name, installed/latest versions and manual update
   guidance; do not block playback or run updates from Health. The host owns
   generic localized wording, not provider-specific names.
3. Add focused provider/host tests for both conditions, current and unknown
   versions, and warning clearance after successful or manual repair. Preserve
   N-01 lease, process-cleanup, permission and scheduling behavior.
4. Review and fix concrete P1/P2 findings and update the milestone evidence.
   Use the milestone acceptance to select remaining checks.

Backup, rollback, quarantine, extra replacement validation, manual update controls,
expanded history and new retry infrastructure are withdrawn, not N-03 work.

#### N-02 source and hydration map

- Product extension: no ErsatzTV native add-on updater counterpart. Reuse the
  accepted N-01 coordinator and official provider's native `-U` invocation.
- Provider: `libexec/tool-update.ts` records bounded release evidence in the
  existing `ERSATZRS_ADDON_DATA_DIR`; `addon.check.v1` owns the warning.
- Host: `addon_tool_maintenance.rs` refreshes the existing bounded readiness
  check after recording a completed update attempt; `AddonPlatformHealthCheck`
  projects persisted `LastCheckStatus`, `LastCheckCode` and optional validated
  warning details in one nullable `LastCheckToolUpdateJson` column. The host
  advertises `ERSATZRS_ADDON_CHECK_CONTEXT_VERSION=1`; older hosts retain the
  three-field provider check response. No update-result wire change.
- Route `/settings/addons/:id`: existing `AddonDetailPage` SSR wrapper and
  `AddonDetailIsland`; `last_check_code`/`last_check_message` and the optional
  `last_check_tool_update` DTO field drive localized text in the existing
  status paragraph. No new endpoint,
  server function, control, DOM subtree, CSS, or shared primitive.
- The same status text is server-rendered without JavaScript. Health uses its
  existing message localization path. Browser proof target:
  `tests/e2e/addon-tool-maintenance.spec.ts` (hydrated and no-JS).


## N-03 execution sequence

1. Map each still-unmet acceptance clause to the candidate revision and existing
   tests/captures. Inspect retained artifacts before reuse; recover missing
   evidence before repeating work. Keep browser output in separate directories.
2. Run only outstanding gates and fix demonstrated failures. Use existing
   contract, migration, publication/recovery, refresh, warning and browser suites.
   Add a regression only for a demonstrated defect or an uncovered requirement.
3. Preserve packaged Windows scheduled-scan/publication/disconnect/restart/UI
   recovery proof with one exact final revision, causal diagnostics and no
   stranded journal or manual repair. Use a disposable candidate and synthetic
   data. Seeded recovery is not a substitute for packaged Windows execution.
4. Verify the exact provider bundle's Linux x64 and Windows x64 entrypoints,
   imported helpers, permissions and v2 metadata/content-kind behavior. Include
   the former NFO/local-replacement closeout under N-03. Static bundle inspection
   is not native execution. Other platforms, MySQL and retired Beeld & Geluid
   acceptance remain excluded by the recorded amendments.
5. Preserve real-provider SponsorBlock MPEG-TS/HLS and primary/fallback human-
   subtitle viewing proof. Reuse valid captures; synthetic subtitles and
   container-only TSDuck checks do not prove content playback. Preserve the
   approved ten-second target/twenty-second maximum and uninterrupted cuts.
6. Exercise source/change/reset/history, inherited options, publication recovery,
   disk warnings and the failed-and-stale updater warning in hydrated and no-JS
   flows. Coordinate disposable browser-profile guidance and NTFS fault proof.
7. Keep N-03 open until every remaining clause is proven. Run builds/media checks
   serially with bounded concurrency. Exact-SHA CI is not manual playback or
   Windows dogfood proof. Release procedures and authorization remain separate
   from this documentation cleanup; use the owning tracker and release guide.

Use the existing [Windows hardening plan](windows-beta-release-hardening.md)
for package acceptance. Do not add transport, updater, storage, test or release
infrastructure merely to make an existing checklist easier to satisfy.
