# Leptos runtime configuration in release builds

ErsatzRS release archives build the frontend and server in two different steps:

- `cargo leptos build --release --frontend-only --lib-features hydrate` produces
  the browser-side Leptos JS/WASM files under `site/pkg`.
- `cargo build --release --bin ersatzrs-web ...` produces the native server
  binary.

The native server binary is not built by `cargo leptos`. That means it cannot
rely on cargo-leptos-provided compile-time defaults when it later runs on a
user's machine. The release launcher supplies the runtime configuration through
environment variables instead.

The critical value is `LEPTOS_OUTPUT_NAME=ersatzrs-web`. The HTML shell derives
the hydration script and WASM names from that output name. If it is empty or
wrong, the server can still render HTML, but the browser does not load the
matching hydration scripts and interactive admin UI actions silently stop
working.

The frontend assets and server binary also need to come from the same commit.
Leptos server functions include generated identifiers in the browser bundle and
server routes. Mixing a server binary from one commit with `site/pkg` from
another commit can produce missing asset requests or server-function 404s even
when both artifacts were valid separately.

The [release-bundle reference](../reference/release-bundle.md) lists the files
that must be staged together.
