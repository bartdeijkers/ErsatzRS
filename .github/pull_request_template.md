## Summary

## Type of Change

- [ ] Feature
- [ ] Fix
- [ ] Docs
- [ ] Refactor
- [ ] Test
- [ ] CI/build

## Contracts

- [ ] I followed the relevant contract files.
- [ ] I cited the upstream ErsatzTV source paths used for this change.
- [ ] I added or linked an ADR if this changes architecture or deliberately diverges from ErsatzTV.
- [ ] I updated `CHANGELOG.md`.
- [ ] I updated `docs/progress/` when parity status changed.

## Parity Review

- [ ] Source map covers UI/pages, components/view models, validators, handlers/services, repository/query paths, and side effects.
- [ ] No hidden stopgaps: any incomplete behavior is listed under `ParityDebt` with a blocker, ADR/deviation, or next-slice owner.
- [ ] Visible UI fields/actions/routes, validation behavior, persistence, and delete/copy/edit side effects match ErsatzTV or have an ADR.
- [ ] Hydration fallback is verified for critical save/action flows, or hydrated-only behavior is explicitly tested.
- [ ] Health/troubleshooting impact was considered when runtime requirements, provider availability, schema readiness, media state, or FFmpeg capability changed.
- [ ] Cross-platform assumptions are guarded with `#[cfg(...)]` or documented as intentional.

## Testing

- [ ] Tests were added or updated.
- [ ] Snapshots were reviewed when generated output changed.
- [ ] `cargo xtask parity-check <scope>` was run where applicable.
- [ ] `cargo fmt --all -- --check` was run where applicable.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` was run where applicable.
- [ ] `./run.sh test-closeout` was run where applicable.
- [ ] `cargo deny check` was run where applicable.

## Notes
