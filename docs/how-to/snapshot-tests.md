# Run snapshot tests

Snapshot tests are mandatory when generated command or timeline output is the
behavior under test.

## Required uses

- Every FFmpeg argv generator.
- Every filter-complex builder.
- Every YAML instruction handler.
- Every drawvg VGS/ASS template renderer.

## Workflow

```bash
cargo test
cargo insta review
```

Review each snapshot as behavior, not as a formatting artifact. Never commit
`.snap.new` files.

## Naming

Use names that identify the scenario:

```text
software_pipeline_single_video_hls
yaml_duration_instruction_exact_fit
drawvg_lower_third_with_title
```

## Updates

Snapshot updates must be paired with an explanation in the pull request. If the
change is a deliberate ErsatzTV behavior divergence, link the ADR.
