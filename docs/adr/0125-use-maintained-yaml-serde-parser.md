# ADR 0125: Use the maintained YAML Serde parser

## Status

Accepted (2026-10-14). Originally accepted as ADR0124 in the isolated UCU-10
branch; renumbered to0125 to avoid the independently reserved teletext ADR0124.
The accepted decision is unchanged.

## Implementation

Implemented (local branch `feat/ucu-10-v26.11.2`, 2026-10-14).

The four package aliases and resolved lockfile pass the full dependency audit,
affected Linux and native Windows compatibility checks, strict lint, workspace
closeout, matched SSR/WASM assembly, headed browser checks and actual Native/Next
playback gates. UCU-10 owns the evidence. The separately approved Next-only
distribution is published and its cold-start checks pass; this application
implementation is not pushed or published.

## Context

The local UCU-10 dependency audit fails on RUSTSEC-2024-0449, issued on
2026-10-12. `unsafe-libyaml` 0.2.11 is archived and has no patched version.
It is pulled in by `serde_yaml` 0.9.34, which is also deprecated. This is a
maintenance finding; it does not establish an observed playback exploit.

Four crates use this parser: core, infrastructure, scanner and web. Their
existing contracts include YAML scheduling, nested filler continuity,
Remote Stream definitions and playlist publication, MPEG-TS script loading,
graphics import and troubleshooting. They use both typed Serde decoding and
the YAML `Value`/`Mapping` API. A parser replacement must preserve these
contracts, including international tracks and persistent scheduling state.

The YAML organization publishes `yaml_serde` as a maintained fork with a
documented Cargo package-renaming migration. Published version 0.10.7 uses
`libyaml-rs` 0.3, requires Rust 1.82 and is MIT OR Apache-2.0 licensed. Its
registry archive checksum is
`33b729a08a9a6be689bbad3e2bf8015926db54b6622cc89c3a5f7dc174b9e918`.
The replacement backend is still a Rust translation of libyaml; this decision
does not claim that it contains no unsafe code.

## Decision

Replace the four `serde_yaml` package dependencies with `yaml_serde` 0.10.7
using the existing dependency name:

```toml
serde_yaml = { package = "yaml_serde", version = "0.10.7" }
```

Commit the exact resolved registry versions and checksums in `Cargo.lock`.
Retain the existing call sites and public YAML models. Change application code
only for a demonstrated compatibility difference, preserving the existing
serialized contract and rejection behavior. Do not introduce a second parser,
a fallback to the archived backend or new workspace unsafe-code allowances.

Acceptance requires:

- The resolved application graph contains neither `serde_yaml` nor
  `unsafe-libyaml`; the complete dependency audit passes without a new ignore.
- Existing typed and value-based YAML tests pass, preserving tags, scalar and
  mapping semantics, duplicate-field rejection and generated definitions.
  Investigate a failed expectation before changing any fixture or snapshot.
- Nested pre/mid/post-roll filler, cycle refusal, serialized Continue,
  KeepCurrent Refresh and SQLite reopen preserve accepted selections and clocks.
- Remote Stream parse/export/rescan, managed playlist publication, graphics
  import, troubleshooting and MPEG-TS script loading retain their existing
  behavior, including selected audio/subtitles and source identity.
- Strict affected lint, available native Windows checks and the final UCU-10
  local gates pass. Existing approved hardware/platform debt stays explicitly
  unverified; no new acceptance is deferred by this decision.

This corrects the demonstrated UCU-10 audit blocker. It does not expand the
upstream catch-up window beyond v26.11.2 or change either streaming engine.
It grants no push, GitHub CI or publication authority.

## Consequences

The application uses an actively maintained parser backend while retaining
the existing Serde and YAML value interfaces. The fork's compatibility claim
must be verified against ErsatzRS tests; it is not acceptance evidence itself.
Error wording or serialization differences may require investigation and a
small correction. Runtime and persisted-state requirements remain unchanged.

## Alternatives Considered

- Ignore the maintenance advisory: leaves the failed agreed audit unresolved.
- Keep `serde_yaml_ng`: it still depends on the archived `unsafe-libyaml`.
- Move to `serde-saphyr`: offers a different parser and parsing budgets, but
  requires a broader migration of the existing value/mapping interfaces.
  The maintained compatible fork is the smaller correction for this blocker.
- Vendor or rename the archived code: does not provide maintained code or
  establish a correction for the reported maintenance finding.

## References

- [ADR 0001](0001-record-architecture-decisions.md)
- [UCU-10 acceptance and evidence](../progress/milestones.toml)
- [Upstream catch-up parity and audit debt](../progress/parity/upstream-baseline-catch-up.toml)
- [YAML scheduling parity](../progress/parity/yaml-scheduling.toml)
- [RustSec RUSTSEC-2024-0449](https://rustsec.org/advisories/RUSTSEC-2024-0449.html)
- [YAML Serde migration and license](https://github.com/yaml/yaml-serde)
- [YAML Serde package manifest](https://github.com/yaml/yaml-serde/blob/main/Cargo.toml)
- [libyaml-rs package manifest](https://github.com/yaml/libyaml-rs/blob/main/Cargo.toml)
