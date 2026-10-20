//! One pinned actual editor save retains the complete generated Base prefix.
use super::*;

const ORIGINAL: [u32; 3] = [16681, 213_960, 16383];
const AUTHORED: [u32; 7] = [16681, 213_960, 16682, 36872, 166_472, 14345, 16383];

fn assert_words(bytes: &[u8], expected: &[u32]) {
    let words: Vec<_> = main_words(bytes)
        .into_iter()
        .flat_map(|record| match record {
            TeletextTtiRecord::Packet { words, .. } => words.to_vec(),
            _ => unreachable!(),
        })
        .collect();
    assert!(words.len() >= expected.len());
    assert_eq!(&words[..expected.len()], expected);
    assert!(words[expected.len()..].iter().all(|word| *word == 0x3fff));
}

fn assert_actual(
    request: &NativeTeletextEditorialImportRequest,
    expected: &NativeTeletextEditorialImportRequest,
    name: &str,
) {
    assert_eq!(request.sidecar, expected.sidecar);
    assert_eq!(request.files.len(), expected.files.len());
    for wanted in &expected.files {
        let actual = &request
            .files
            .iter()
            .find(|file| file.name == wanted.name)
            .unwrap()
            .bytes;
        if wanted.name != name {
            assert_eq!(actual, &wanted.bytes);
            continue;
        }
        assert_words(actual, &AUTHORED);
        let a = parse_teletext_tti(actual, policy().tti).unwrap();
        let b = parse_teletext_tti(&wanted.bytes, policy().tti).unwrap();
        // Existing QTM writer grouping: unchanged X28 precedes X26/base rows
        // and FastLinks moves. Every complete record within each group is exact.
        for group in 0..3 {
            let selected = |record: &&TeletextTtiRecord| {
                let actual = match record {
                    TeletextTtiRecord::FastLinks(_) => 0,
                    TeletextTtiRecord::Packet { row: 28, .. } => 1,
                    _ => 2,
                };
                actual == group
            };
            assert_eq!(
                a.records().iter().filter(selected).collect::<Vec<_>>(),
                b.records().iter().filter(selected).collect::<Vec<_>>()
            );
        }
    }
}

pub(super) async fn edit(
    directory: &std::path::Path,
    flow: &Flow,
    expected: &NativeTeletextEditorialImportRequest,
    program: &ersatzrs_domain::TeletextEnhancementProgram,
) -> NativeTeletextEditorialImportRequest {
    use super::super::super::local_object_glyphs::actual_editor as qtm;
    let name = &flow.exported[0].sidecar().project.pages[0].name;
    let original = &flow.exported[0]
        .files()
        .iter()
        .find(|file| &file.name == name)
        .unwrap()
        .bytes;
    let target = &expected
        .files
        .iter()
        .find(|file| &file.name == name)
        .unwrap()
        .bytes;
    assert_words(original, &ORIGINAL);
    assert_words(target, &AUTHORED);
    qtm::inputs_for_layout(
        directory,
        "single-page-create-main-generated-base",
        flow,
        [4, 12, 12],
        || {
            json!({
                "kind":"main_presence_generated_base", "before":null, "after":program,
                "consumer_visual_columns":[12], "original_words":ORIGINAL,
                "generated_base_prefix":[16681,213_960], "expected_words":AUTHORED
            })
        },
    );
    live_client::write_new(&directory.join("expected-edited.tti"), target).unwrap();
    let _ = qtm::completion(directory).await;
    let bytes =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let request = qtm::actual_request(directory, flow, &bytes);
    assert_actual(&request, expected, name);
    request
}
