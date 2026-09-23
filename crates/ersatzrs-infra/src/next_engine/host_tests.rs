use super::*;

fn request(preview: bool) -> NextStart {
    NextStart {
        channel_id: 1,
        preview,
        number: "1".into(),
        executable: "fixture-next".into(),
        output: if preview { "preview" } else { "normal" }.into(),
        ffmpeg: "fixture-ffmpeg".into(),
        ffprobe: "fixture-ffprobe".into(),
        replacements: Vec::new(),
        idle_timeout: None,
        deadline: Instant::now() + Duration::from_secs(30),
    }
}

#[test]
fn dynamic_context_survives_either_normal_or_preview_session_stopping() {
    for first_preview in [false, true] {
        let mut contexts = HashMap::new();
        let first = request(first_preview);
        let second = request(!first_preview);
        retain_dynamic_context(&mut contexts, &first);
        retain_dynamic_context(&mut contexts, &second);
        // The callback configuration remains the initial shared configuration,
        // even when another registry starts the same channel number.
        assert_eq!(contexts["1"].request.output, first.output);
        release_dynamic_context(&mut contexts, &first.number);
        let surviving = &contexts["1"].request;
        assert_eq!(surviving.channel_id, second.channel_id);
        assert_eq!(surviving.ffmpeg, second.ffmpeg);
        assert_eq!(surviving.number, second.number);
        release_dynamic_context(&mut contexts, &second.number);
        assert!(contexts.is_empty());
        // A later session owns fresh configuration, not an expired predecessor.
        let mut replacement = request(false);
        replacement.ffmpeg = "replacement-ffmpeg".into();
        retain_dynamic_context(&mut contexts, &replacement);
        assert_eq!(contexts["1"].request.ffmpeg, replacement.ffmpeg);
        release_dynamic_context(&mut contexts, &replacement.number);
        assert!(contexts.is_empty());
    }
}
