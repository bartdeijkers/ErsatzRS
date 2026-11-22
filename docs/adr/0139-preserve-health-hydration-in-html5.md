# ADR 0139: Preserve Health hydration in HTML5

## Status

Proposed

## Implementation

Implemented

## Context

The Leptos islands renderer emits empty comment nodes as `<!>` and appends
resource scripts after the document. Browsers recover both constructs, but
the Nu HTML Checker reports errors. The client needs those comment nodes at
their original positions and consumes the resource scripts in order.

## Decision

Normalize the streamed GET response for the System Health document in the web
host. Use the native-only `lol_html` tokenizer to serialize empty comments and
omit the optional body and html end tags. The browser consequently parses the
trailing resource scripts inside the body. Preserve script text, attributes and
the original stream ordering. Other routes keep their existing renderer.

## Consequences

Validation and hydration tests must use the same server and WASM build.
The response incurs HTML tokenization proportional to its size and can still
start rendering before the last chunk arrives.

## Alternatives Considered

Removing markers would change the DOM that the island traverses. String
replacement could also change script or attribute contents. Rebuilding a full
document tree would buffer the complete report before sending it.

## References

- [Health response handling](../../crates/ersatzrs-web/src/host/health_document.rs)
- [Hydration and browser tests](../../TEST.md)
