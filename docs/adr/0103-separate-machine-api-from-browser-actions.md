# ADR 0103: Separate machine API authentication from browser actions

## Status

Accepted — maintainer approval on 2026-09-27, with `X-Ers-Api-Key` as the header.

## Implementation

Implemented (UCU-07 row 112, 2026-09-27) — persisted-key storage,
startup/router integration, OpenAPI and browser request-path migration have
focused tests, strict lint and matched Chromium/Firefox evidence. The approved
`X-Ers-Api-Key` header has final admission, old-header rejection and listener
isolation proof. Milestone closeout and exact-commit CI remain separate obligations.

## Context

Upstream commit `c27a8d5031c30bb3557a959db55a959ab2aa2aae` requires
`X-Etv-Api-Key` on machine requests under `/api`. Browser troubleshooting
retains the management policy, which requires a UI session when upstream OIDC
is configured. Streaming and internal callbacks retain their own policies.

ErsatzRS currently registers native management forms, uploads, browser JSON
actions and Leptos server functions under `/api`, alongside the machine artwork
cleanup endpoint. Sending the machine key to browser code would disclose it.
Allowing arbitrary browser requests to bypass a shared API guard would weaken
the machine API boundary. ADR 0102 already separates listener surfaces.

## Decision

Reserve `/api` for machine requests and require `X-Ers-Api-Key`.
The maintainer explicitly chose the ErsatzRS header name instead of upstream
`X-Etv-Api-Key`; the upstream name is not accepted as an alias.
Move browser-only handlers and generated Leptos server functions to
`/admin-api`. Update their callers together, including native forms and static
JavaScript. Do not retain anonymous aliases at the former `/api` paths.
Keep `/api/troubleshoot` as the upstream's segment-delimited management
exception; similarly named prefixes must not bypass machine authentication.

Apply authentication before routing or body extraction for the complete `/api`
namespace, including unknown paths and unsupported methods. Accept exactly one
nonempty header with exact, constant-time value comparison. Cookies, query
parameters, Host and forwarded headers do not authorize machine API requests.
Authentication failure returns an empty unauthorized response without echoing
credentials. Mark the header sensitive before request tracing.

Persist a cryptographically random 32-byte base64url key in `api-secrets.json`
under the durable application state directory, using the upstream `ApiKey`
JSON property. Preserve an existing valid key across restarts. Create new files
without exposing partial content or overwriting another startup's winning key;
use owner-only Unix permissions and the application directory's Windows access
controls. Missing or invalid configuration may be initialized, but read or write
failure must never open the API. Never log key values or return them through
the admin UI. Document restart-based rotation and header requirements.
Startup aborts with a redacted error if credential initialization fails, rather
than presenting a partially available server. This differs from upstream's
continued startup with all machine requests denied and makes readiness truthful.

Keep the management surface's existing trusted-network deployment contract.
This decision does not claim that ErsatzRS implements upstream OIDC sessions.
Operators requiring management login use an authenticated reverse proxy and
prevent direct access to the management listener. Retaining that deployment
boundary instead of adding upstream's built-in OIDC is an explicit
maintainer-approved adaptation. Machine
keys do not authorize or protect the management surface. Publishing a
streaming-only listener must not expose either machine APIs or browser actions,
even with a valid API key.

The scanner's existing database-backed coordination and in-process Lua
scheduling do not make the upstream scanner/Python HTTP calls. Preserve these
transports rather than introduce HTTP solely to carry a key. Any external
HTTP consumer must supply the header; no key enters process arguments.

## Consequences

Machine clients must add the header. Internal browser action URLs change and
require a matched SSR/WASM build. No credential is embedded in HTML, JavaScript,
hydration data or URLs. Public IPTV authentication and private callback
capabilities remain independent of the machine key.

Acceptance requires persisted-key lifecycle tests, production-router denial and
side-effect tests, exact troubleshooting-prefix tests, shared/separate listener
tests, and matched hydrated/no-JS browser coverage of affected route families.
Existing route inventories and contracts must detect stale action paths.
OpenAPI descriptions for exposed machine endpoints must declare the key scheme.

## Alternatives considered

- Give browser code the machine key: exposes a reusable machine credential.
- Exempt individual browser paths inside `/api`: makes new routes depend on a
  growing security allowlist and obscures the public machine contract.
- Authenticate only the currently known machine handler: leaves future `/api`
  handlers outside the mandatory policy by default.
- Add scanner HTTP and external Python scheduling: introduces transports that
  the current architecture does not need.

## References

- [Listener separation](0102-isolate-http-listener-route-surfaces.md)
- [Catch-up source map](../../plans/upstream-catch-up.md)
- [Architecture](../../ARCHITECTURE.md)
