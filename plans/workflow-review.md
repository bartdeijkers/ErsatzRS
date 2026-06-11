# Workflow review: parity-progress pipeline

Reviewed: 2026-06-11. Trigger: the 2026-06-10 runtime triage of
`ErsatzRS.log`, which forced six parity features back from `Ported` to
`In progress` in a single day. This document reviews the workflow that is
supposed to prevent exactly that, and recommends improvements. It is a
review with recommendations, not a contract; each recommendation becomes
binding only when implemented in the referenced files.

Implementation status: R1-R4 were implemented on 2026-06-11. The binding
contract changes now live in `AGENTS.md`, `TEST.md`,
`docs/automation/daily-progress-routine.md`,
`docs/automation/gap-prompt-rules.md`, `docs/automation/runtime-triage.md`,
`docs/progress/parity/*.toml`, `skills/`, and the `cargo xtask progress` /
`cargo xtask smoke` implementations.

## Scope and method

Reviewed surfaces:

- the progress index and its tooling (`docs/progress/`, `xtask/src/progress.rs`,
  `xtask/src/parity_check.rs`);
- the automation contracts ([parity-baseline.md](../docs/automation/parity-baseline.md),
  [daily-progress-routine.md](../docs/automation/daily-progress-routine.md),
  [gap-prompt-rules.md](../docs/automation/gap-prompt-rules.md),
  [../top-three-gaps-to-fill-prompts.md](../docs/top-three-gaps-to-fill-prompts.md));
- the verification layers (`TEST.md`, `.github/workflows/ci.yml`, snapshot and
  fixture tests under `crates/ersatzrs-ffmpeg/tests/`);
- the agent entrypoints (`AGENTS.md`, `.claude/skills/`, `.codex/skills/`).

Method: code-level exploration of the xtask implementation, cross-checking the
automation docs against what actually exists, and the 2026-06-10 runtime
triage (log clustering → parity-slug mapping → dated ParityDebt entries).

## What works and must be kept

- **Text as source of truth, SQLite as derived index.** `docs/progress/*.toml`
  and `sessions.ndjson` are authoritative; `progress.db` is gitignored and
  auto-rebuilt on mtime by `cargo xtask progress sync`. Queries
  (`next`, `gaps`, `parity`, `routes`, `hydration`, `sessions`, `adr`) are fast
  and agent-agnostic because they are plain cargo commands.
- **Validation in CI.** `cargo xtask progress check` gates status enums, ADR
  references, and file structure on every push and PR.
- **Pinned parity baseline.** [parity-baseline.md](../docs/automation/parity-baseline.md) keeps
  gap scope fixed to one upstream commit; the daily routine cannot advance it.
- **ADR reverse-links.** Parity files reference ADR numbers; the derived index
  answers "which features deviate under ADR NNNN" without manual upkeep.
- **Single agnostic entrypoint.** `AGENTS.md` is the workflow contract;
  `CLAUDE.md` and `CODEX.md` only point at it. Any LLM that can run shell
  commands can drive the whole loop.
- **The no-stopgap / ParityDebt convention.** Incomplete behavior must be
  visible debt instead of silent omission. The convention is right; the
  enforcement is what is missing (see findings 1 and 3).

## Findings

### 1. `Ported` is not evidence-backed

Nothing in the schema or in `cargo xtask progress check` ties
`status = 'Ported'` to any proof (test name, fixture, runtime observation).
The cost became visible on 2026-06-10: six features
(`collections-smart-collections`, `ffmpeg-filter-graph`,
`ffmpeg-command-generation`, `scanner`, `classic-scheduling`, `search-index`)
were rolled back to `In progress` because real playback, scanning, and search
contradicted snapshot-test-only validation — including a CUDA filter graph
that cannot start at all and an AV1 decode fallback whose fix had already
been recorded as done the day before.

### 2. No automated runtime verification

The strongest evidence the project produces — TSDuck transport analysis and
HLS segment sampling — exists only as manual narrative inside
`docs/progress/parity/hls-variant-playlists-segmenter.toml`. CI runs fmt,
clippy, tests, and `progress check`, but never boots the server, never spawns
FFmpeg, and never requests a channel. Every failure class found on 2026-06-10
(filter-graph format mismatch, hardware-decode fallback not effective,
segmenter startup timeout) is invisible to the current gate ladder and only
surfaces when a human watches a channel.

### 3. ParityDebt is narrative-only and summaries balloon

`ParityDebt` is a writing convention inside the free-text `summary`, so no
tool can list open debts, count them, or fail a `Ported` claim that still has
one. Summaries also accumulate history:
`docs/progress/parity/ffmpeg-command-generation.toml` is a single ~12 KB
narrative block of stacked "This slice also…" paragraphs. Every agent that
touches the feature pays that context cost, and the current state is buried
in the changelog-style prose that git history already records.

### 4. The contracts are not fully LLM-agnostic

- Nine skills are byte-identical duplicates in `.claude/skills/` and
  `.codex/skills/`, and they have already drifted: `parity-slice.md` exists
  only under `.codex/skills/`. The generated gap prompts tell every agent —
  including Claude — to use `.codex/skills/parity-slice.md`.
- UI verification in `TEST.md` and the gap prompts leans on Playwright **MCP**
  tool calls (`browser_navigate`, `browser_snapshot`), which only exist in
  Claude Code with that MCP server attached. A Codex session cannot execute
  those steps; the equivalent agnostic form is a runnable
  `npx playwright test` spec.

### 5. Minor doc/reality drift

- The mechanism that actually triggers
  [daily-progress-routine.md](../docs/automation/daily-progress-routine.md) (scheduler, account,
  cadence owner) is not documented anywhere in the repo.
- `phase.toml` checklist items flip to `done = true` only by hand; nothing
  reconciles them.
- The gap-prompt template has no explicit "does this slice deviate from
  upstream → is an ADR required?" checkpoint, even though the ADR rule is the
  core of "ADR-respecting".

## Recommendations

Complexity is expressed in points (1 = trivial, 8 = substantial), not time.
Suggested execution order: R1 → R2 → R3 → R4.

### R1 — Make the contracts LLM-agnostic (2 points)

- Keep one canonical skills directory (for example `skills/` at the repo
  root, referenced from `AGENTS.md`); reduce `.claude/skills/` and
  `.codex/skills/` to thin pointer files so the sets cannot drift again.
- Add `parity-slice.md` to the canonical set and fix the
  [../top-three-gaps-to-fill-prompts.md](../docs/top-three-gaps-to-fill-prompts.md)
  references through [gap-prompt-rules.md](../docs/automation/gap-prompt-rules.md).
- Rule for prompts and `TEST.md`: every verification step must be a plain
  command any agent can run (`cargo xtask …`, `npx playwright test …`,
  `tsp …`). MCP-driven browsing may remain as a convenience, never as the
  only documented path.

### R2 — Summary hygiene and a runtime-triage routine (3 points)

- `cargo xtask progress check` warns when a parity `summary` exceeds a length
  budget (proposal: 4,000 characters). The `summary` describes the current
  state; slice-by-slice history belongs in `sessions.ndjson` and git log.
  Trim the worst offenders (`ffmpeg-command-generation`, `scanner`) when next
  touched.
- Add `docs/automation/runtime-triage.md` codifying the procedure used on
  2026-06-10: cluster WARN/ERROR lines in `ErsatzRS.log`, map each cluster to
  a parity slug, record a dated ParityDebt entry quoting the exact log
  evidence, and downgrade the status when the debt contradicts `Ported`.
  This makes user-observed runtime failures a first-class input to the
  progress index instead of an ad-hoc rescue.

### R3 — Evidence-gated status (5 points)

- Extend the parity TOML schema with an optional structured debt array:

  ```toml
  [[debt]]
  id          = 'cuda-filter-format-mismatch'
  description = 'NVENC/CUDA graph emits unconvertible filter chains at runtime'
  discovered  = '2026-06-10'
  evidence    = 'ErsatzRS.log: Impossible to convert between the formats ...'
  ```

- `cargo xtask progress check` fails when `status = 'Ported'` while open
  `[[debt]]` entries exist; `cargo xtask progress gaps` lists individual
  debts, giving the daily routine finer-grained prompt candidates than
  whole-feature slugs.
- Migrate incrementally: narrative ParityDebt stays valid; new debts use the
  structured form, and existing ones move over when a feature is next
  touched.

### R4 — Runtime smoke gate (8 points)

- New `cargo xtask smoke`: boot the server against a small fixture library,
  request `/iptv/channel/<n>.ts` and the HLS playlist for each smoke channel,
  require initial segments within the startup timeout, and fail on nonzero
  FFmpeg exits or known failure signatures in the log (segmenter exit loops,
  `Impossible to convert between the formats`, hardware-decode fallback
  errors).
- Optional TSDuck pass (available on this machine on both Windows and WSL)
  for transport-level assertions, mirroring the manual proof already recorded
  in `hls-variant-playlists-segmenter.toml`.
- Smoke output is exactly the `evidence` R3 wants: a passing smoke run is
  what justifies flipping a streaming-path feature to `Ported`, replacing
  manual narrative proof. Run it locally and as a scheduled job on the
  self-hosted runner; it does not need to gate every PR.

### Minor follow-ups (1 point each)

- Document where and how the daily routine is scheduled.
- Add an "ADR needed?" checkpoint to the gap-prompt template in
  [gap-prompt-rules.md](../docs/automation/gap-prompt-rules.md).
- Require a `sessions.ndjson` entry whenever a parity status changes, so
  status flips carry their rationale.

## Dependency note

R1 and R2 are independent quick wins. R3 and R4 reinforce each other: R3
defines where evidence lives, R4 produces it. Implementing R3 first is fine —
debts can cite manual log triage until the smoke gate exists — but `Ported`
claims on streaming-path features stay trust-based until R4 lands.
