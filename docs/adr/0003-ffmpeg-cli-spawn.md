# ADR 0003: Invoke FFmpeg through the CLI

## Status

Accepted

## Implementation

Implemented — the FFmpeg pipeline spawns the CLI via `tokio::process::Command`.

## Context

ErsatzTV builds FFmpeg command lines and runs FFmpeg as a child process. ErsatzRS
needs to preserve those pipeline patterns while using FFmpeg 8.1 features.

## Decision

Use `tokio::process::Command` to spawn FFmpeg. Do not use libav bindings.

## Consequences

ErsatzRS can mirror ErsatzTV's argv and filter-graph structure closely and can move
between FFmpeg builds without binding churn. Process supervision, logging, and
argument escaping must be handled carefully.

## Alternatives Considered

- libav bindings: rejected because they increase coupling and diverge from
  ErsatzTV's proven process model.
- Shell invocation: rejected because arguments must be structured and safe.

## References

- [../../PORTING.md](../../PORTING.md)
