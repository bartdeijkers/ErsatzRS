# Releasing

ErsatzRS has no releases yet. This policy applies once release automation is
enabled.

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
2. Confirm the parity matrix reflects the release contents.
3. Move `[Unreleased]` entries in `CHANGELOG.md` into a dated version section.
4. Create a new empty `[Unreleased]` section.
5. Tag the release.
6. Let the release workflow create artifacts.
7. Verify the GitHub release notes match the changelog.
