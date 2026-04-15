# Project Status

Last updated: 2026-04-15

## Current Phase

**Phase 0 — Bootstrap** (partially complete)

### Phase 0 Checklist

- [x] Documentation contracts (CONTRIBUTING, STYLE, ARCHITECTURE, PORTING)
- [x] ADRs 0001–0007
- [x] Diataxis documentation structure
- [x] Parity matrix skeleton
- [x] GitHub CI, issue templates, PR template, dependabot
- [x] Tooling config (rustfmt, clippy, deny, editorconfig, gitattributes)
- [x] Claude skills for recurring workflows
- [x] Conventional Commits and commitlint
- [ ] Cargo workspace and empty crate stubs
- [ ] FFmpeg 8.1 runtime detection and fail-fast
- [ ] Minimal axum + Leptos SSR page
- [ ] SQLite bootstrap (empty DB on first run)
- [ ] Docker image with FFmpeg 8.1
- [ ] Health endpoint (`/healthz` → 200)

### What an Agent Should Do Next

If resuming Phase 0 code work:

1. Read [plans/sequential-dancing-valiant.md](plans/sequential-dancing-valiant.md)
   Phase 0 and its verification criteria.
2. Read [ARCHITECTURE.md](ARCHITECTURE.md) for the crate layout.
3. Create root `Cargo.toml` workspace with empty crate stubs under `crates/`.
4. Add FFmpeg 8.1 version detection in `nastv-web`.
5. Add a minimal axum health endpoint.
6. Add SQLite pool creation at startup.
7. Verify with the Phase 0 checks from the plan.

### Blocking Decisions

None. All Phase 0 decisions are recorded in ADRs 0001–0007.

## Session Log

Record brief handoff notes when stopping a work session.

| Date | Agent | Summary |
| --- | --- | --- |
| 2026-04-15 | Copilot (Claude) | Audited repo readiness; added CODEX.md, STATUS.md, copilot-instructions.md, Commands section in AGENTS.md. |
