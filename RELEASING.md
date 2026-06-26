# Releasing

ErsatzRS publishes alpha releases while Phase 7 parity work continues. The
release workflow builds Linux and Windows binary archives from annotated version
tags.

## Versioning

Before `1.0.0`, ErsatzRS uses semver with this convention:

- Breaking changes increment the minor version.
- Compatible features increment the minor version.
- Fixes increment the patch version.

After `1.0.0`, standard semver applies.

## Tag Format

Use annotated tags:

```bash
git tag -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0
```

## Release Checklist

1. Confirm CI is green.
2. Confirm `docs/progress/` reflects the release contents.
3. Move `[Unreleased]` entries in `CHANGELOG.md` into a dated version section.
4. Create a new empty `[Unreleased]` section.
5. Add or update the compare links at the bottom of `CHANGELOG.md`.
6. Tag the release.
7. Let the release workflow create the Linux and Windows binary archives.
8. Verify the GitHub release notes match the changelog, which is the canonical
   release overview.

For the command sequence, see [Publish a full release](docs/how-to/publish-release.md).

## Windows bundle (`ersatzrs-<version>-win-x64.zip`)

The win-x64 archive contains the launcher, server, scanner, hydration assets,
and application assets. It does **not** bundle FFmpeg/ffprobe. On first startup,
ErsatzRS downloads and validates the matching first-party
[`ErsatzRS-ffmpeg`](https://github.com/bartdeijkers/ErsatzRS-ffmpeg) release into
the application state directory, and later starts reuse or update that managed
runtime. See [Release Bundle Reference](docs/reference/release-bundle.md) for
the archive layout and launcher environment.

For local Windows release publishing, use
[Build and publish a local Windows release](docs/how-to/release-locally.md).

The launcher delivers a console `Ctrl+C` to the server on Exit; the server's
`host::bootstrap::shutdown_signal` turns that into a graceful drain.

## Code signing (Windows) — optional, not yet enabled

Unsigned Windows binaries trigger SmartScreen "unknown publisher" warnings. The
release workflow includes an **optional signing step that is skipped by default**
(it only runs when the `AZURE_CLIENT_ID` secret is set). Until then, releases are
unsigned and users must click through SmartScreen.

### Options (researched June 2026)

| Approach | Cost | Individual-eligible? | Clears SmartScreen instantly? |
|---|---|---|---|
| **Azure Artifact Signing** (formerly Trusted Signing / Azure Code Signing) | ~$9.99/mo (Basic, 5,000 sigs) | Public-Trust certs: **individuals only in US/Canada**; EU individuals must enroll as an **organization** (e.g. a KvK-registered business) | No — reputation builds over download history |
| **OV cert + Azure Key Vault HSM** (DigiCert/Sectigo/SSL.com, signed via AzureSignTool/signtool) | ~$200–400/yr | Yes | No — reputation builds over time |
| **EV cert** | ~$300–700/yr | No — requires a registered org + hardware token | **Yes** — clears SmartScreen instantly |

ErsatzTV uses Azure Artifact Signing via `azure/trusted-signing-action@v0` with an
OIDC Azure login. The workflow mirrors that.

### One-time setup to enable signing

1. Create an **Azure Trusted Signing** account and a **certificate profile**
   (named `ErsatzRS` to match the workflow `certificate-profile-name`).
2. Register a GitHub OIDC **federated credential** on an Azure app registration
   (Entra ID) scoped to this repository, with the **Trusted Signing Certificate
   Profile Signer** role on the account.
3. Add repository secrets: `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`,
   `AZURE_SUBSCRIPTION_ID`.
4. Confirm the workflow's `endpoint` region (`https://eus.codesigning.azure.net/`)
   matches the Trusted Signing account region.

Once the secrets exist, the `Azure login for signing` and `Sign Windows binaries`
steps in the `binaries` job run automatically for the win-x64 build (signing
`ErsatzRS.exe`, `ersatzrs-web.exe`, `ersatzrs-scanner.exe`, and
`ersatzrs-healthcheck.exe` before they are zipped).
No workflow change is needed to turn signing on.
