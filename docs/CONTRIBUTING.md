# Documentation contributing

Documentation is part of the product. It follows the same English-only rule as
code and commit messages.

## Style

- Use English.
- Use sentence-case headings.
- Use relative links for repository files.
- Use fenced code blocks with language tags.
- Avoid raw HTML unless Markdown cannot express the content.
- Keep lines readable, but do not wrap tables in ways that make them harder to
  edit.
- Prefer concrete examples over vague advice.
- Keep [CHANGELOG.md](../CHANGELOG.md) as the main release overview. Changelog
  entries follow Keep a Changelog categories, are curated for users and
  maintainers, and are not raw commit-log summaries.

## Diátaxis placement

- Put learning paths in `docs/tutorials/`. A tutorial should state what the
  reader will build, choose one reliable path, and finish with a working result.
- Put task guides in `docs/how-to/`. Start from a concrete goal, state required
  prerequisites, and give the shortest reliable procedure. Link to explanation
  instead of interrupting the task with design history.
- Put lookup material in `docs/reference/`. Keep names, defaults, paths,
  precedence, supported values, and interfaces complete and easy to scan.
- Put background and tradeoffs in `docs/explanation/`. Build a mental model or
  explain causes and consequences; link to a how-to guide for action steps.
- Put architecture decisions in `docs/adr/`.

Do not duplicate the same procedure across categories. A tutorial may link to a
how-to guide after teaching the first complete path, and a how-to guide may link
to reference for exact values.

## End-user accuracy

- Verify UI labels and routes against the current admin route and navigation
  sources.
- Verify release artifacts against the current release workflow and published
  release before recommending an installation path.
- Distinguish current beta availability from the broader normal-release matrix.
- Use placeholders for host names, tokens, and media paths. Never include
  maintainer-machine paths or real credentials.
- Mark internal endpoints and unstable implementation details as such.
- Keep `docs/README.md` focused on end-user learning and operation. Add
  contributor-only pages to the map below instead of putting them in the
  primary end-user journey.

## Contributor documentation map

Start with the repository contracts that govern the kind of change:

- [Development setup and contribution workflow](../CONTRIBUTING.md)
- [Build and development commands](../BUILD.md)
- [Code and SQL style](../STYLE.md)
- [Architecture contract](../ARCHITECTURE.md)
- [ErsatzTV porting workflow](../PORTING.md)
- [Admin UI and hydration testing](../TEST.md)
- [Current structured progress](progress/README.md)
- [Plan and decision index](../plans/INDEX.md)
- [Architecture decision records](adr/)

Contributor how-to guides:

- [Add an FFmpeg pipeline](how-to/add-ffmpeg-pipeline.md)
- [Add a migration](how-to/add-migration.md)
- [Run tests](how-to/testing.md)
- [Run snapshot tests](how-to/snapshot-tests.md)
- [Reproduce an HLS content-timing deviation](how-to/reproduce-hls-content-timing-deviation.md)

Release-maintainer how-to guides:

- [Publish a full release](how-to/publish-release.md)
- [Publish a release without GitHub-hosted minutes](how-to/publish-release-with-self-hosted-runners.md)
- [Build and publish a local Windows release](how-to/release-locally.md)

Contributor reference:

- [Parity matrix](reference/parity-matrix.md)
- [Persistence gap matrix](reference/persistence-gap-matrix.md)
- [Web crate structure](reference/web-crate-structure.md)
- [Remote Stream playlist manifest contract](reference/remote-stream-playlist-manifests.md)

Engineering explanation:

- [Deliberate deviations from ErsatzTV](explanation/deviations.md)
- [Test strategy](explanation/test-strategy.md)
- [Leptos runtime configuration](explanation/leptos-runtime-config.md)
- [Avoiding the inspection paradox in streaming-reliability testing](explanation/streaming-reliability-sampling.md)
- [HLS segmenter playback deviation root cause](explanation/hls-segmenter-playback-deviation-root-cause.md)

## Rustdoc

Public items in `ersatzrs-core`, `ersatzrs-ffmpeg`, and `ersatzrs-app` need rustdoc.
Examples are expected when an API is stable enough to demonstrate safely.

## Links

Link to ADRs when a document depends on a decision. Link to
[PORTING.md](../PORTING.md) when a document describes ErsatzTV parity work.
