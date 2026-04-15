# Documentation Contributing

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

## Diataxis Placement

- Put learning paths in `docs/tutorials/`.
- Put task guides in `docs/how-to/`.
- Put lookup material in `docs/reference/`.
- Put background and tradeoffs in `docs/explanation/`.
- Put architecture decisions in `docs/adr/`.

## Rustdoc

Public items in `ersatzrs-core`, `ersatzrs-ffmpeg`, and `ersatzrs-app` need rustdoc.
Examples are expected when an API is stable enough to demonstrate safely.

## Links

Link to ADRs when a document depends on a decision. Link to
[PORTING.md](../PORTING.md) when a document describes ErsatzTV parity work.
