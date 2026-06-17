# MPEG-TS troubleshooting with TSDuck

> **Status: Completed / historical.** Shipped as ADR 0013 plus the TSDuck
> playback gate (`scripts/e2e/tsduck-playback-gate.sh`). Kept for context.

## Summary

Add an optional MPEG-TS diagnostic path that uses
[TSDuck](https://tsduck.io/) to capture and analyze bounded samples from
ErsatzRS transport-stream output. This is troubleshooting-only: normal IPTV,
HLS, HDHomeRun, FFmpeg spawning, and startup readiness must not depend on
TSDuck.

The first implementation target is a Support tool at
`/system/troubleshooting/mpeg` for channel-level MPEG-TS analysis. It should
produce a structured report and archive that make packet-level stream defects
visible: PSI/SI structure, PAT/PMT/service/PID mapping, continuity counter
errors, PCR validity, and bitrate estimates.

This is an intentional ErsatzRS diagnostic extension, not an ErsatzTV parity
port. A local source search in `$ERSATZTV_SRC` found no `TSDuck`, `tsduck`,
`tsp`, `tsanalyze`, `tsbitrate`, `pcrverify`, `tsfixcc`, or `tsfclean`
usage.

## Installing TSDuck

TSDuck is a developer/operator-side tool only; it is not bundled with ErsatzRS
(see [ADR 0013](../docs/adr/0013-tsduck-mpeg-troubleshooting.md)). It is not in
the Ubuntu apt repositories, so install the upstream `.deb` directly. For other
platforms, download the matching artifact from the
[releases page](https://github.com/tsduck/tsduck/releases/latest).

### WSL / Ubuntu 24.04 (amd64)

```bash
# Resolve the latest ubuntu24 amd64 deb URL from the GitHub releases API
URL=$(curl -sL https://api.github.com/repos/tsduck/tsduck/releases/latest \
  | grep -oE '"browser_download_url": *"[^"]+\.deb"' \
  | sed 's/.*"\(http[^"]*\)"/\1/' \
  | grep -E 'tsduck_.*ubuntu24_amd64\.deb')

curl -fL -o /tmp/tsduck_amd64.deb "$URL"

# Use apt-get install (not dpkg -i) so runtime deps resolve: libcurl, libsrt, ...
sudo apt-get install -y /tmp/tsduck_amd64.deb
```

### Verify

```bash
tsp --version        # e.g. "version 3.44-4676"
tsanalyze --version
```

The diagnostic service detects `tsp --version` at runtime; a missing TSDuck
yields `NotAvailable` and never fails startup or `/healthz`.

## Source map

- ErsatzRS stream surfaces:
  `crates/ersatzrs-web/src/transport/iptv/stream.rs`,
  `crates/ersatzrs-web/src/transport/iptv/hls.rs`,
  `crates/ersatzrs-web/src/transport/iptv/variant.rs`,
  `crates/ersatzrs-infra/src/hls_worker.rs`, and HDHomeRun routes that
  ultimately point clients at IPTV stream URLs.
- ErsatzRS troubleshooting surfaces:
  `crates/ersatzrs-web/src/admin/pages/system/troubleshooting.rs`,
  `/system/troubleshooting/playback`, and the existing troubleshooting archive
  pattern from the ErsatzTV source.
- ErsatzRS health surfaces:
  `ersatzrs-core::health`, `ersatzrs-infra::health`, and persisted
  `ConfigElement` keys `health.results` / `health.summary`.
- TSDuck official references:
  `tsp`, `tsanalyze` / `analyze`, `continuity`, `pcrverify`, `tsbitrate`,
  `tsfixcc`, and `tsfclean`.

## Implementation changes

- Add a small TSDuck diagnostic service in infra/app code that owns all command
  construction and parsing.
  - Configure binary paths through settings or environment, defaulting to
    `tsp`, `tsanalyze`, and `tsbitrate`.
  - Detect `tsp --version`; missing TSDuck returns `NotAvailable` and never
    fails startup or `/healthz`.
  - Use `tokio::process::Command` with structured arguments only. Do not pass
    user-provided TSDuck arguments and do not invoke a shell.
- Add a bounded capture step before analysis.
  - Inputs: channel number, stream mode, requested duration, and raw-sample
    archive opt-in.
  - Default duration: 20 seconds. Maximum duration: 120 seconds.
  - Maximum retained sample size: 512 MiB.
  - Write samples under the existing troubleshooting/temp area with a unique
    run id.
  - Capture only MPEG-TS outputs: direct `.ts` IPTV output and HLS `.ts`
    segments when the selected channel mode/output format is MPEG-TS.
  - Return `NotApplicable` for fMP4, MP4, and MKV outputs in v1.
- Run read-only TSDuck analysis on the captured file.
  - Primary command:

    ```bash
    tsp --realtime=off \
      -I file <sample.ts> \
      -P continuity --json-line=ERSATZRS_CONTINUITY \
      -P pcrverify \
      -P analyze --json --output-file <analysis.json> --error-analysis \
      -O drop
    ```

  - Optional bitrate command:

    ```bash
    tsbitrate --value-only <sample.ts>
    ```

  - `tsbitrate` failure does not fail the report when the primary `tsp`
    analysis completed.
  - Do not run repair or mutation tools in v1: no `tsfixcc`, no `tsfclean`,
    no `pcradjust`, no `pcrcopy`, and no TSDuck output file that replaces an
    ErsatzRS stream.
- Produce a structured report.
  - Include run id, channel, stream mode, capture duration, byte count, sample
    path basename, timestamps, command exit status, and TSDuck version.
  - Include PAT/PMT/service/PID summary from `analyze --json`.
  - Include continuity findings from the `continuity` log or JSON line.
  - Include PCR findings from `pcrverify` output.
  - Include bitrate estimate when `tsbitrate` succeeds.
  - Bound stdout/stderr excerpts and redact URLs, access tokens, bearer tokens,
    and credentials before rendering or archiving.
  - Report status as one of `Pass`, `Warning`, `Fail`, `NotAvailable`, or
    `NotApplicable`.
- Add `/system/troubleshooting/mpeg`.
  - Link it from the existing `/system/troubleshooting` Tools panel.
  - Render an SSR-safe form with channel selector, stream mode selector,
    duration input, and raw-sample archive checkbox.
  - Render the latest report as tables and preformatted diagnostic excerpts.
  - Add translations for every visible label, status, help text, and error in
    `crates/ersatzrs-web/src/admin/languages/` for `en-us`, `pl`, `pt-br`,
    and `nl`.
- Add archive support.
  - Include `summary.json`, `analysis.json`, `continuity.log`,
    `pcrverify.log`, `tsbitrate.txt`, and sanitized command metadata when
    present.
  - Exclude raw `.ts` samples unless the user explicitly selected the raw
    sample opt-in.
  - Delete expired sample/report directories through the existing
    troubleshooting cleanup path or a small cleanup helper in the same slice.
- Add health integration.
  - Add `TSDuckTroubleshootingHealthCheck`.
  - Return `Info` when TSDuck is available and v1 commands can be discovered.
  - Return `NotApplicable` when missing or disabled.
  - Never increment readiness failures and never block startup.

## Test plan

- Unit-test command construction for `tsp` and `tsbitrate`; assert exact argv
  and no shell wrapping.
- Unit-test detection for available, missing, and too-old/unknown TSDuck
  version output.
- Unit-test report parsing for:
  - valid `analyze --json`
  - malformed `analysis.json`
  - continuity warnings
  - PCR warnings
  - missing optional `tsbitrate`
  - bounded stdout/stderr truncation and redaction
- Add focused infra/app tests for `NotAvailable`, `NotApplicable`, `Pass`,
  `Warning`, and `Fail` status mapping.
- Add a small synthetic `.ts` fixture only if it is compact and redistributable;
  otherwise mock process output and keep raw media samples out of the repo.
- Add web tests for the Support tool route:
  - `/system/troubleshooting` links to MPEG-TS troubleshooting.
  - `/system/troubleshooting/mpeg` renders the SSR form.
  - missing TSDuck renders a clear `Not available` state.
  - fMP4/MP4/MKV modes render `Not applicable`.
- Run:

  ```bash
  cargo fmt --all -- --check
  cargo check -p ersatzrs-web --features ssr
  cargo test -p ersatzrs-infra tsduck --all-features
  cargo test -p ersatzrs-web troubleshooting --features ssr
  ```

## Assumptions

- TSDuck remains optional for v1 and is not bundled into Docker images until a
  separate packaging review accepts the image-size, license, and multi-arch
  costs. The maintainer's dev box (WSL Ubuntu 24.04) runs TSDuck 3.44-4676 at
  `/usr/bin/tsp`; see "Installing TSDuck" above for the install steps.
- The first useful diagnostic target is channel MPEG-TS output. Other
  containers and deeper HLS playlist analysis can be separate follow-up slices.
- Raw stream samples may contain user media and must be treated as sensitive.
  Reports can be archived by default after redaction; raw samples require
  explicit opt-in.
- The implementation follows [ADR 0013](../docs/adr/0013-tsduck-mpeg-troubleshooting.md)
  once accepted.
