# Playback FFmpegProfile Loading Parity Slice

Closes the gap where ErsatzRS live playback ignores the channel's configured
**FFmpegProfile** and hardcodes a fixed transcode target. Today both playback
command builders create `DesiredVideoState::h264_1080p()` regardless of the
channel — so every channel is forced to H.264 / 1080p / 8-bit / 8 Mbps / AAC
stereo / software encode, and the profile's resolution, codec, bitrate, bit
depth, hardware acceleration, frame-rate and loudness normalization are all
dropped. This slice makes playback build its `DesiredVideoState` (and pick its
pipeline builder) from the channel's `FFmpegProfile`, matching ErsatzTV.

This builds on the 2026-05-30 pixel-format fix: the source video stream is now
probed from stored `MediaStream` rows (`ersatzrs_infra::playback::load_source_video_stream`)
and drives the `SetPixelFormat` port in `pipeline/mod.rs`. That plumbing stays;
this slice replaces the hardcoded *desired* side.

## Source Map

- **ErsatzRS (to change):**
  - `crates/ersatzrs-web/src/transport/iptv/stream.rs` — `build_stream_command`
    (MPEG-TS / `.ts` + hls-direct), uses `DesiredVideoState::h264_1080p()`.
  - `crates/ersatzrs-infra/src/hls_worker.rs` — `build_hls_command` (HLS
    segmenter), same hardcode.
  - `crates/ersatzrs-ffmpeg/src/pipeline/mod.rs` — `DesiredVideoState`
    (`frame_size`, `frame_rate`, `video_format`, `audio_format`, `bit_depth`,
    `video_profile`, `video_preset`, `video_bitrate`, `video_buffer_size`,
    `audio_bitrate`, `audio_channels`, `audio_sample_rate`, `allow_b_frames`)
    and the `*PipelineBuilder` family (Software/Nvenc/Vaapi/Qsv/VideoToolbox/
    Amf/V4l2m2m/Rkmpp).
  - `crates/ersatzrs-domain/src/ffmpeg.rs` — the `FFmpegProfile` domain type.
  - `crates/ersatzrs-web/src/transport/iptv/streaming_specs.rs` —
    `load_spec`/`ChannelStreamingSpec` already loads a **subset** of the profile
    (video/audio format, width/height, video/audio bitrate) for M3U codec tags;
    extend or generalize this into a full profile loader to avoid a second
    query path.
- **ErsatzTV (authoritative):**
  - `ErsatzTV.Application/Streaming/Queries/GetPlayoutItemProcessByChannelNumberHandler.cs`
    and `GetSlugProcessByChannelNumber.cs` — resolve channel → playout item →
    `FFmpegProfile`, then call the process service.
  - `ErsatzTV.Core/FFmpeg/FFmpegPlaybackSettings.cs` — computes the desired
    output state from the profile + channel + source stream (scaled/padded size
    via `ScalingBehavior`/`PadMode` + `Resolution`, frame-rate normalization,
    bit depth, video/audio codec/profile/preset/bitrate/buffer, audio
    channels/sample rate, b-frames, thread count).
  - `ErsatzTV.Core/FFmpeg/FFmpegLibraryProcessService.cs` /
    `FFmpegProcessService.cs` — select the pipeline builder from
    `FFmpegProfile.HardwareAcceleration` and assemble the command.
  - `ErsatzTV.Core/Domain/FFmpegProfile.cs` — field reference (Resolution,
    ScalingBehavior, PadMode, VideoFormat, VideoProfile, VideoPreset,
    AllowBFrames, BitDepth, VideoBitrate, VideoBufferSize, TonemapAlgorithm,
    AudioFormat, AudioBitrate, AudioChannels, AudioSampleRate, NormalizeAudio,
    NormalizeLoudnessMode/TargetLoudness, ThreadCount, HardwareAcceleration,
    Vaapi*/QsvExtraHardwareFrames).

## Current state vs. target

- **Now:** `DesiredVideoState::h264_1080p()` hardcoded in both builders;
  `SoftwarePipelineBuilder` always used. Observed consequence on the Star Trek
  channel: output stayed at the source `1440x1080` (the desired `1920x1080` was
  never applied because `FrameState` is seeded from the desired size and the
  scale filter is therefore skipped), regardless of the channel's profile.
- **Target:** load the channel's `FFmpegProfile`, map it to `DesiredVideoState`,
  seed `FrameState` from the **source** dimensions so scale/pad actually run,
  pick the pipeline builder from `HardwareAcceleration`, and pass runtime
  capabilities so HW acceleration can downgrade to software when unsupported.

## Approach

1. **Profile loader.** Add a profile loader keyed by the channel's
   `FFmpegProfileId` (generalize `streaming_specs::load_spec` or add a sibling
   that returns the full `FFmpegProfile`). Both builders already have the
   channel/`media_version_id` in hand from the active-item query.
2. **Map `FFmpegProfile` → `DesiredVideoState`.** Add a conversion (e.g.
   `DesiredVideoState::from_profile(&FFmpegProfile)`), porting
   `FFmpegPlaybackSettings`: resolution/`ScalingBehavior`/`PadMode` →
   `frame_size`, frame-rate normalization → `frame_rate`, `BitDepth` →
   `bit_depth`/pixel format, video/audio codec/profile/preset/bitrate/buffer,
   audio channels/sample rate, `AllowBFrames`, `ThreadCount`.
3. **Seed source frame size.** Populate `FrameState`/the request with the source
   `width`/`height` (from `MediaVersion`/`MediaStream`, already loaded via
   `load_source_video_stream`) so `scale_filter`/`pad_filter` engage instead of
   no-opping against the desired size.
4. **Select the pipeline builder** from `FFmpegProfile.HardwareAcceleration`
   (Software/Nvenc/Vaapi/Qsv/VideoToolbox/Amf/V4l2m2m/Rkmpp) and pass
   `with_runtime_capabilities(...)` so `effective_video_acceleration` can fall
   back to software when encode/decode is unsupported (mirrors ErsatzTV).
5. **Extend `SetPixelFormat` to hardware builders** (currently software-only) so
   the parity fix holds across accelerations — ErsatzTV adds
   `PixelFormatOutputOption` in every builder.

## Parity considerations / risk

- The hardcoded `h264_1080p` desired frame rate `30000/1001` currently forces
  30fps re-timing if a source frame rate is supplied; profile-driven frame-rate
  normalization must follow `FFmpegProfile`/`NormalizeFramerate` semantics, not
  a constant.
- Changing the desired state will change generated argv; update the
  `pipeline_snapshots.rs` insta snapshots intentionally and add profile-driven
  cases (e.g. HEVC 10-bit profile, 720p scale-and-pad, AC-3 audio).
- Hardware builders are untested at runtime here; keep software as the default
  and gate HW selection behind capability detection so a misconfigured profile
  degrades gracefully rather than failing the stream.
- Loudness/audio normalization (`NormalizeLoudnessMode`, `NormalizeAudio`) and
  tonemap algorithm selection are part of the profile; scope whether they land
  in this slice or a follow-up.

## Verification

- `cargo test -p ersatzrs-ffmpeg` (snapshots updated + new profile-driven cases)
  and `cargo test -p ersatzrs-infra`.
- `cargo check -p ersatzrs-web --no-default-features --features ssr`.
- End-to-end: with a non-default profile (e.g. 720p HEVC or AC-3 audio), play
  the channel via the HLS segmenter; fetch a segment and `ffprobe` it to confirm
  the codec/resolution/bit-depth/audio match the profile (the method used to
  verify the 10-bit→8-bit fix).

## Scope / risk

- Touches both playback builders, the `DesiredVideoState` mapping, and pipeline
  builder selection — medium risk because it changes every channel's transcode
  command. Snapshot review + an ffprobe-on-segment smoke per profile variant is
  the gate.
- Independent of `plans/media-sources-hydration-anomaly.md`.
