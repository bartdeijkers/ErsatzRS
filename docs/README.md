# Documentation

ErsatzRS documentation follows Diataxis. Every document should fit one of four
purposes.

## Tutorials

Tutorials teach by walking through a complete path. They assume the reader wants
to learn the system, not merely complete a task.

Location: [tutorials/](tutorials/)

## How-to Guides

How-to guides solve one practical problem. They should be short, direct, and
task-oriented.

Location: [how-to/](how-to/)

Current how-to guides:

- [Publish a full release](how-to/publish-release.md)
- [Publish a release without GitHub-hosted minutes](how-to/publish-release-with-self-hosted-runners.md)
- [Build and publish a local Windows release](how-to/release-locally.md)

## Reference

Reference pages are lookup material. They should be complete, structured, and
stable.

Location: [reference/](reference/)

Current reference pages:

- [Configuration](reference/configuration.md)
- [Glossary](reference/glossary.md)
- [Parity matrix](reference/parity-matrix.md)
- [Release bundle](reference/release-bundle.md)

## Explanation

Explanation pages discuss background, tradeoffs, and reasons.

Location: [explanation/](explanation/)

Current explanation pages:

- [Leptos runtime configuration](explanation/leptos-runtime-config.md)
- [Avoiding the inspection paradox in streaming-reliability testing](explanation/streaming-reliability-sampling.md)

## Rule of Thumb

- Do not put "why" material in a how-to guide.
- Do not put step-by-step task instructions in an explanation page.
- Do not use tutorials as reference dumps.
- Do not hide policy in reference pages when contributors need it in
  [docs/CONTRIBUTING.md](CONTRIBUTING.md).
