use super::*;

#[test]
fn consumer_probe_reuses_builder_flags_and_keeps_ordinary_probe_unchanged() {
    let extractor = FfprobeExtractor::new("synthetic-probe");
    let source = std::path::Path::new("synthetic-source.ts");
    let ordinary = extractor.source_origin_command(source).unwrap();
    assert!(!ordinary.args.iter().any(|arg| arg == "-fflags"));
    let consumer = extractor.consumer_origin_command(source).unwrap();
    let expected: Vec<_> = ersatzrs_ffmpeg::standard_format_flags()
        .global_options()
        .iter()
        .cloned()
        .chain(ordinary.args.clone())
        .collect();
    assert_eq!(consumer.program, ordinary.program);
    assert_eq!(consumer.args, expected);
    assert_eq!(extractor.source_origin_command(source).unwrap(), ordinary);
    assert!(!consumer.args.iter().any(|arg| matches!(
        arg.as_str(),
        "-ss" | "-copyts" | "-read_intervals" | "-select_streams" | "-nofind_stream_info"
    )));
}
