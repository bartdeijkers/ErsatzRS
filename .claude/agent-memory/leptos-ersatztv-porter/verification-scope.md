---
name: Verification scope — web crate only, no Playwright in loop
description: Phase 4 contract for the porter — what to run inside the agent loop, what to hand off to the human at commit time.
type: feedback
---

The porter's Phase 4 verification runs **only** these commands inside the
agent loop:

1. `cargo xtask hydration-lint -p ersatzrs-web`
2. `cargo build -p ersatzrs-web`
3. `cargo clippy -p ersatzrs-web --all-targets -- -D warnings`

Everything else — `cargo build --workspace`, `cargo clippy --all-features`,
`cargo test --workspace`, `cargo leptos build`, Playwright MCP — is handed
off to the human as a pre-commit checklist in the final report.

**Why:** the May 2026 movies-page port took ~100 minutes and 251 tool calls
because each iteration ran the full workspace gates plus a Playwright MCP
session. The actual code change was small. The user explicitly asked to
shift the balance toward coding by demoting verification gates that the
human will rerun at commit anyway. The hydration-lint xtask absorbs the
specific quality risk (broken hydration via `<Show>` in islands or
prop-bearing islands across `<Suspense>`) that previously needed a
Playwright round-trip to catch.

**How to apply:**

- Never run `cargo leptos build` inside your loop unless the user explicitly
  asks for a Playwright verification. Hand off the command instead.
- Never drive Playwright MCP yourself unless the user asks. The smoke test
  is a human responsibility.
- Always include a "Pre-commit gates for the user" section at the end of
  your final report. The exact list is in [[leptos-ersatztv-porter.md]]
  Phase 4 step 14.
- If you change code outside `ersatzrs-web` (e.g. add a server fn that
  needs an app-layer handler), flag it explicitly so the human knows to
  widen their test scope.
- The hydration linter has a baseline file at
  `.hydration-lint-baseline` listing pre-existing findings. When you touch
  a baselined file and your fix resolves the finding, delete its line. If
  you can't fix it in scope, leave the line — but never silently delete
  a line for a finding you didn't actually fix.

Reference fix: the May 2026 channels editor refactor (commit after movies
page) demonstrates the pattern for moving from "page passes props to
island inside `<Suspense>`" to "page passes id only, island owns
`Resource::new_blocking`, inner `#[component]` renders the form inside
the island's `<Suspense>`". See [[movies-page-port.md]] for the broader
pattern family.
