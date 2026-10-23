//! Complete native/default/fallback witnesses and bounded refusal controls.
use super::*;
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, NativeTeletextEditorialTtiFailure,
};

pub(super) fn qualified(
    old: &EncodedTeletextAuthoredService,
    wanted: &EncodedTeletextAuthoredService,
) {
    assert_eq!(old.declared().semantic().objects().definitions().len(), 5);
    assert_eq!(
        display(old)
            .normalized()
            .encoded()
            .definition_positions()
            .len(),
        1
    );
    assert_eq!(
        display_page(old, 125)
            .normalized()
            .encoded()
            .definition_positions()
            .len(),
        1
    );
    assert_eq!(
        old.declared().semantic().base(),
        wanted.declared().semantic().base()
    );
    assert_eq!(
        old.declared().semantic().objects(),
        wanted.declared().semantic().objects()
    );
    assert_eq!(
        old.declared().resource_inventory(),
        wanted.declared().resource_inventory()
    );
    assert_eq!(old.magazine_units(), wanted.magazine_units());
    for number in [124, 125, 126] {
        let old_display = display_page(old, number).normalized().original();
        let wanted_display = display_page(wanted, number).normalized().original();
        assert_eq!(
            old_display.original_program_source(),
            wanted_display.original_program_source()
        );
        assert_eq!(fallback(old, number), fallback(wanted, number));
        assert_eq!(
            old.declared().semantic().page_enhancements()[&channel_documents::address(number, 32)]
                .resources
                .defaults,
            seed::defaults()
        );
    }
    let default = display_page(old, 126).normalized().original();
    assert_eq!(
        default.original_program_source(),
        &TeletextEffectiveProgramSource::Defaults {
            slots: vec![0, 2, 3]
        }
    );
    let view = default.display_resolution();
    let origin = view
        .cell(4, default.display_geometry().left_width() + 1)
        .unwrap()
        .origin()
        .unwrap();
    let candidate = default.glyph_layers().candidates()[view.origins()[origin].candidate_index()];
    assert_eq!(
        candidate.root_source(),
        Some(TeletextSpatialRootSource::DefaultSlot { slot: 3 })
    );
    assert_eq!(candidate.source().unwrap().object, Some(seed::target(21)));
    assert!(candidate.invocation().is_some());
    let before = designs::units(old);
    let after = designs::units(wanted);
    assert_ne!(before[0], after[0]);
    assert_eq!(&before[1..], &after[1..]);
}

fn fallback(service: &EncodedTeletextAuthoredService, number: u16) -> Vec<Vec<u8>> {
    display_page(service, number)
        .normalized()
        .original()
        .base_display()
        .cells()
        .iter()
        .map(|row| row.iter().map(|cell| cell.code()).collect())
        .collect()
}

pub(super) fn request(flow: &Flow, target: &Flow) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let name = &flow.exported[0].sidecar().project.pages[0].name;
    let original = request
        .files
        .iter_mut()
        .find(|file| &file.name == name)
        .unwrap();
    let wanted = target.exported[0]
        .files()
        .iter()
        .find(|file| &file.name == name)
        .unwrap();
    let protected = |bytes: &[u8]| {
        parse_teletext_tti(bytes, policy().tti)
            .unwrap()
            .records()
            .iter()
            .filter(|record| !matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(protected(&original.bytes), protected(&wanted.bytes));
    original.bytes.clone_from(&wanted.bytes);
    request
}

fn mutate(
    request: &mut NativeTeletextEditorialImportRequest,
    word: usize,
    f: impl FnOnce(u32) -> u32,
) {
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let words = records
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words,
            } if usize::from(*designation) == word / 13 => Some(words),
            _ => None,
        })
        .unwrap();
    let before = words[word % 13];
    words[word % 13] = f(before);
    assert_ne!(before, words[word % 13]);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
}

async fn refused(fixture: &Fixture, flow: &Flow, request: NativeTeletextEditorialImportRequest) {
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Tti(error) = error else {
        panic!("complete packet qualification must reject corruption: {error:?}")
    };
    assert!(matches!(
        error.failure,
        NativeTeletextEditorialTtiFailure::Packet | NativeTeletextEditorialTtiFailure::Structure
    ));
    assert_eq!((error.page, error.subcode), (124, 32));
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap(),
        Some(flow.saved[0].head().clone())
    );
    assert_original_pointers(fixture, flow).await;
}

pub(super) async fn negatives(fixture: &Fixture, flow: &Flow, target: &Flow) {
    let native = display(target.accepted[0].service()).normalized().encoded();
    let invocation = native
        .command_positions()
        .iter()
        .find(|position| position.original_local_command() == Some(1))
        .unwrap();
    let mut bad_pointer = request(flow, target);
    mutate(&mut bad_pointer, invocation.semantic_word(), |word| {
        word ^ (1 << 11)
    });
    refused(fixture, flow, bad_pointer).await;
    let tail = native
        .command_positions()
        .iter()
        .find(|position| position.source().object.is_some())
        .unwrap();
    let mut bad_tail = request(flow, target);
    mutate(&mut bad_tail, tail.semantic_word(), |word| word ^ (1 << 11));
    refused(fixture, flow, bad_tail).await;
    let mut truncated = request(flow, target);
    let file = truncated
        .files
        .iter_mut()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            for (slot, word) in words.iter_mut().enumerate() {
                if usize::from(*designation) * 13 + slot > native.main_triplet_count() {
                    *word = 12287;
                }
            }
        }
    }
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    refused(fixture, flow, truncated).await;
    if let Some(restore) = native.command_positions().iter().find(|position| {
        matches!(
            position.original_main_source(),
            Some(
                ersatzrs_ffmpeg::teletext::TeletextMainCompositionSource::BaseCell {
                    position:cell,
                    kind: ersatzrs_ffmpeg::teletext::TeletextBaseCellCommandKind::CharsetRestore,
                }
            ) if cell.row()==6 && cell.column()==8
        )
    }) {
        stale_restore(fixture, flow, target, restore.semantic_word()).await;
    }
}

async fn stale_restore(fixture: &Fixture, flow: &Flow, target: &Flow, word: usize) {
    for mixed_base in [false, true] {
        let mut stale = request(flow, target);
        mutate(&mut stale, word, |word| {
            assert_eq!(word >> 11, 0x37);
            (word & 0x7ff) | (0x57 << 11)
        });
        if mixed_base {
            let file = stale
                .files
                .iter_mut()
                .find(|file| file.name == "page-124-0020.tti")
                .unwrap();
            let mut records = parse_teletext_tti(&file.bytes, policy().tti)
                .unwrap()
                .records()
                .to_vec();
            let cells = records
                .iter_mut()
                .find_map(|record| match record {
                    TeletextTtiRecord::BaseRow { row: 6, cells } => Some(cells),
                    _ => None,
                })
                .unwrap();
            assert_eq!(cells[12], b' ');
            cells[12] = b'X';
            file.bytes = write_teletext_tti(
                &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
                policy().tti,
            )
            .unwrap();
        }
        refused(fixture, flow, stale).await;
    }
}
