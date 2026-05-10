# ADR 0002: Use Leptos for the admin UI

## Status

Accepted

## Context

ErsatzRS needs a Rust-native web UI that can start server-rendered and gain
interactivity where needed.

## Decision

Use Leptos with SSR and selective hydration. Mount it through `leptos_axum` in
the `ersatzrs-web` crate.

## Consequences

The project stays in Rust across server and UI code. The cost is that MudBlazor
components from ErsatzTV do not have a one-to-one replacement and some UI
components must be built or adapted.

## Alternatives Considered

- Blazor: rejected because ErsatzRS intentionally leaves the .NET stack.
- Yew: rejected because SSR and full-stack routing are less aligned with the
  desired host model.
- Server-only HTML: rejected because later admin workflows need interactivity.

## References

- [Project contracts and status](../../plans/INDEX.md)
- [Current phase status](../../STATUS.md)
