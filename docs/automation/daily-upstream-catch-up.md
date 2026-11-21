# Daily upstream catch-up routine (UCU)

This is the contract for the scheduled agent that keeps ErsatzRS caught up with
upstream [ErsatzTV/legacy](https://github.com/ErsatzTV/legacy). It runs once a
day and also turns every new upstream release into an ErsatzRS release.
Edit this file to change the routine; the schedule points at this contract.

- **Schedule:** daily at 06:47 Europe/Amsterdam. Upstream usually releases on
  Friday and sometimes ships patch releases between those.
- **Ledger:** [`plans/upstream-catch-up.md`](../../plans/upstream-catch-up.md),
  ordered table `| # | upstream sha | subject (PR) | verdict | owning parity slug | ErsatzRS commit | notes |`.
- **Pin:** the Pin table in [`parity-baseline.md`](parity-baseline.md) remains
  the only authoritative baseline. Its rules about advancing the pin still apply.
- **Delivery:** at most **one** open UCU pull request from this routine at a
  time, with auto-merge enabled. CI changes always go in a separate PR.
  The `CI ok` ruleset on `main` stays in force.
- **Quiet by default:** when there are no new upstream commits and no open
  release step, make no edits and send no report.

## Hard guardrails

1. **Never decide alone** on rows that need a maintainer verdict. These are
   security or advisory issues, keep-legacy and Native-engine retention choices
   (for example row 271 or row 272), architecture or ADR deviations, new
   dependencies, removal of user-visible behavior, and new features larger than
   a small single-domain change. Register them with verdict `Pending (maintainer)`
   and add them to the decision bundle.
2. Never push tags. Cloud sessions cannot push tags, and tags are the
   maintainer's release act. Give the maintainer exact commands instead.
3. Never skip, quarantine or weaken tests or acceptance requirements. Never
   shorten existing teletext or Playwright checks. Run Playwright only when the
   change affects browser-visible behavior, never in parallel.
4. Do not touch rows, pins or Next candidates that another active thread owns.
   This applies until the Pin table reaches the work that thread finishes.
   Today that is rows ≤ 309, Next v0.2.7 (`ucu16-next-row305`) and the pin to
   v26.13.0.
5. Stop after two red CI runs on the same PR. Find the cause before you push
   again, and report it if it is not a defect of the PR itself. Do not merge
   `main` into the branch unless there is a conflict. Self-hosted runners may be
   offline at night, so a waiting job is not a failure.
6. Follow `AGENTS.md`: English repository content, Conventional Commits
   (scopes limited to `commitlint.config` such as `docs`, `status`, `web`, `ci`, `release`),
   CHANGELOG `[Unreleased]` for user-visible fixes, parity records for debt,
   and cleanup of task-owned branches after merge.

## 1. Detect new upstream work

1. `git fetch origin main` in ErsatzRS. Clone or fetch ErsatzTV/legacy, including tags,
   into `$ERSATZTV_SRC`. Do not move a shared checkout that another task uses.
   Use `git show <sha>` for review.
2. Take the last ledger row's sha. List the upstream commits after it in
   first-parent order: `git log --reverse --first-parent --format='%h %s' <last>..origin/main`.
3. List upstream release tags (`v*.*.*`) that point at or contain commits after
   the last tag that has an ErsatzRS release. Each one is a *release target*.
4. If there are no new commits, no unfinished release step from an earlier run
   (§4) and no open routine PR, stop silently.
5. If a routine PR is still open, do not start a new one. Only check its CI.
   Fix a demonstrated defect in it, or report a blocker.

## 2. Register and triage rows

Append one row per new commit, in order, with the 8-character sha and the exact
subject. Read the full diff of each commit before you assign a verdict:

| Verdict | Use when | Who decides |
| --- | --- | --- |
| `N/A (upstream tooling)` | only .NET packages, upstream CI/workflows, branding, release changelog | routine |
| `Covered` | ErsatzRS already behaves this way; cite the code path and an existing or new test | routine |
| `Port` / `Adapt` | small fix in one domain, with focused tests, no new dependency or ADR | routine |
| `Adapt` (Next update) | `chore: update next` rows; record the Next revision and follow §4 | routine prepares, maintainer pushes the Next tag |
| `Pending (maintainer)` | everything in guardrail 1, or any doubt | maintainer |

Put rows the routine decides, and their small fixes, in **one PR per run**
(branch `claude/ucu-daily-<YYYYMMDD>`). If a fix grows beyond a small
single-domain change, register the row as `In progress`. Move the work to its
own PR in a later run and record the plan in the parity record, so that the
docs-only ledger PR stays fast. Update the "observed / processed / awaiting"
summary sentence in the ledger. Run `cargo xtask progress check` when you
change progress records.

## 3. Decision bundle

Collect all `Pending (maintainer)` rows into one numbered list. Give each item
the sha, the subject, what it changes for users, the options, and a recommended
option. Ask once per new item. Do not repeat a question the maintainer already
answered; their answer goes into the row's verdict and notes in the next PR.

## 4. Release follow-through

For every release target from §1, in tag order:

1. **Rows:** all rows up to the tag have a final verdict, except approved
   `Deferred` rows with recorded debt.
2. **Next:** if the window contains Next updates, rebase the Next candidate
   (`scripts/release/next-patches/`, `next-candidate-packages.yml`) onto the
   Next revision of the release. Qualify it in one CI run, then ask the
   maintainer to push the Next release tag. Give the exact commands.
3. **Pin PR:** after the qualified Next release is published, open one PR with
   managed adoption and the Pin table advance to the release commit. Record
   this in `docs/progress/sessions.ndjson`.
4. **Release preparation PR:** bump the workspace version. An upstream minor
   (`v26.N.0`) maps to the next ErsatzRS minor; an upstream patch (`v26.N.P`)
   maps to the next ErsatzRS patch. Move `[Unreleased]` into a dated release
   section and check add-on compatibility bounds. Follow
   [publish-release.md](../how-to/publish-release.md) and the release-candidate
   CI gate in `BUILD.md`.
5. **Tag:** when exact-SHA CI on `main` is green, ask the maintainer to push the
   tag (`git fetch origin main`, `git tag -a vX.Y.Z <sha> -m "ErsatzRS vX.Y.Z"`,
   `git push origin vX.Y.Z`). Then watch the release workflow and report the
   result.

If a step waits on the maintainer, later runs only check whether it is done.
They must not ask again every day.

## 5. Report

Write the report in Dutch, plainly, only when something happened or something
is needed. Cover the new rows (count and verdicts), the PR link and its CI
state, the decision bundle, and the release step that waits on the maintainer.
