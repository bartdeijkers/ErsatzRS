//! Exact immutable bank/selector binding and six literal native cell masks.
use super::fixture::bank_mask;
use super::*;

fn mask(index: u8, target: usize) -> [u16; 10] {
    let TeletextColumnOperation::Drcs { reference } = operation(index, target) else {
        unreachable!()
    };
    bank_mask(reference.table, reference.slot.get())
}

pub(super) fn packet_words(service: &EncodedTeletextAuthoredService) -> [Option<[u32; 13]>; 16] {
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.page())
            }
            _ => None,
        })
        .unwrap();
    let mut result = [None; 16];
    for record in project_encoded_teletext_display(page, service.unit_count())
        .unwrap()
        .records
    {
        if let TeletextEncodedDisplayRecord::Triplets {
            row: 26,
            designation,
            words,
        } = record
        {
            assert!(result[usize::from(designation)].replace(words).is_none());
        }
    }
    result
}

pub(super) fn selectors(service: &EncodedTeletextAuthoredService) -> [usize; 4] {
    let program = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.normalized().encoded())
            }
            _ => None,
        })
        .unwrap();
    let positions = positions(service);
    let words = packet_words(service);
    std::array::from_fn(|target| {
        let position = program
            .command_positions()
            .iter()
            .find(|position| position.semantic_word() == positions[target])
            .unwrap();
        assert!(
            position.first_word() < position.semantic_word(),
            "actual selector and glyph are distinct final emitted words"
        );
        let first = position.first_word();
        assert_eq!(
            words[first / 13].unwrap()[first % 13],
            if matches!(target, 0 | 2) {
                0x28628
            } else {
                0x08628
            },
            "original S1=0 selection, mode24 and exact scope stay protected"
        );
        first
    })
}

fn assert_banks(service: &EncodedTeletextAuthoredService) {
    assert!(service.compiler_drcs_resources().is_empty());
    assert_eq!(service.declared().semantic().drcs().subtables().len(), 2);
    assert_eq!(service.declared().mots().len(), 1);
    let mot = TeletextResourceIdentity::new(
        TeletextResourceKind::Mot,
        TeletextRawDataPageNumber::new(1, 0xfe).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    assert_eq!(service.declared().mots()[0].identity(), mot);
    assert_eq!(service.declared().selections().len(), 2);
    assert!(service
        .declared()
        .selections()
        .iter()
        .all(|selection| selection.mot == Some(mot)));
    for scope in [
        TeletextDrcsTableScope::Normal,
        TeletextDrcsTableScope::Global,
    ] {
        let table = service
            .declared()
            .semantic()
            .drcs()
            .subtables()
            .iter()
            .find(|table| table.identity() == bank(scope))
            .unwrap();
        assert_eq!(table.glyphs().len(), 2);
        for slot in 0..2 {
            assert_eq!(table.glyphs()[usize::from(slot)].slot.get(), slot);
            assert_eq!(
                table.glyphs()[usize::from(slot)].bitmap.get(),
                bank_mask(scope, slot)
            );
        }
        for metadata in service.declared().semantic().page_enhancements().values() {
            assert_eq!(
                metadata.resources.drcs.page(scope),
                Some(bank(scope).page())
            );
        }
    }
}

fn assert_bindings(service: &EncodedTeletextAuthoredService, index: u8) {
    assert_banks(service);
    selectors(service);
    let composition = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.normalized().original())
            }
            _ => None,
        })
        .unwrap();
    let resolution = composition.display_resolution();
    for (visual, target) in VISUAL.into_iter().zip([0, 1, 2, 3, 2, 3]) {
        let origin = resolution
            .cell(1, u8::try_from(visual).unwrap())
            .unwrap()
            .origin()
            .unwrap();
        let candidate = resolution.origins()[origin].candidate_index();
        let executed = composition
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == candidate)
            .unwrap();
        let TeletextColumnOperation::Drcs { reference } = operation(index, target) else {
            unreachable!()
        };
        assert_eq!(executed.reference(), reference);
        assert_eq!(executed.resource(), bank(scope(target)));
        assert_eq!(executed.bitmap().get(), mask(index, target));
    }
}

pub(super) fn assert_pixels(service: &EncodedTeletextAuthoredService, index: u8, night: bool) {
    assert_bindings(service, index);
    let raster = external_editor_probe::preview_page(service, 124);
    assert_eq!((raster.width(), raster.height()), (672, 250));
    for (visual, target) in VISUAL.into_iter().zip([0, 1, 2, 3, 2, 3]) {
        for (row, bits) in mask(index, target).into_iter().enumerate() {
            for x in 0..12 {
                let rgb = if bits & (1 << (11 - x)) == 0 {
                    [0, 0, 0]
                } else if night {
                    [0, 0, 15]
                } else {
                    [15, 0, 0]
                };
                let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(pixel))) =
                    raster.pixel(visual * 12 + x, 10 + row)
                else {
                    panic!("DRCS glyph must have a settled RGB4 pixel");
                };
                assert_eq!(
                    pixel.get(),
                    rgb,
                    "literal existing bank mask; exact full resource/reference binding checked separately"
                );
            }
        }
    }
}

pub(super) fn assert_rendered(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    index: u8,
) {
    assert_eq!(before.pages().len(), after.pages().len());
    assert_eq!(before.magazine_units(), after.magazine_units());
    assert_eq!(before.inventory(), after.inventory());
    for (old, new) in before.pages().iter().zip(after.pages()) {
        assert_eq!(old.resource_header(), new.resource_header());
        let changed = matches!(old,EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address()==channel_documents::address(124,32));
        if changed {
            assert_ne!(old.data_units(), new.data_units());
        } else {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
    assert_eq!(
        external_editor_probe::preview_page(before, 125),
        external_editor_probe::preview_page(after, 125)
    );
    let original = external_editor_probe::preview_page(before, 124);
    let edited = external_editor_probe::preview_page(after, 124);
    let mut changed = [false; 6];
    for (position, (a, b)) in original.pixels().iter().zip(edited.pixels()).enumerate() {
        if a != b {
            let (row, column) = (position / original.width(), position % original.width());
            assert!((10..20).contains(&row));
            let cell = VISUAL
                .iter()
                .position(|visual| *visual == column / 12)
                .expect("protected Main/panel/row pixels stay exact");
            changed[cell] = true;
        }
    }
    assert_eq!(
        changed,
        if index == 1 {
            [true; 6]
        } else {
            [false, true, false, true, false, true]
        }
    );
    assert_pixels(after, index, false);
}

pub(super) fn assert_originals(flow: &Flow) {
    let day = &flow.accepted[0].selection().bindings[0];
    let night = &flow.accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
    for (index, accepted) in flow.accepted.iter().enumerate() {
        let service = accepted.service();
        assert!(service.compiler_drcs_resources().is_empty());
        for owner in [124, 125] {
            let definition = service
                .declared()
                .semantic()
                .objects()
                .definition(identity(owner))
                .unwrap();
            assert_eq!(definition.object_type, TeletextObjectType::Passive);
            assert_eq!(definition.program.commands().len(), 4);
            for target in [2, 3] {
                assert_eq!(
                    definition.program.commands()[COMMANDS[target]],
                    TeletextEnhancementCommand::AtColumn {
                        column: COLUMNS[target],
                        operation: operation(0, target)
                    }
                );
            }
            let commands = service.declared().semantic().page_enhancements()
                [&channel_documents::address(owner, 32)]
                .local_program
                .as_ref()
                .unwrap()
                .commands();
            for target in [0, 1] {
                assert_eq!(
                    commands[COMMANDS[target]],
                    TeletextEnhancementCommand::AtColumn {
                        column: COLUMNS[target],
                        operation: operation(0, target)
                    }
                );
            }
            let calls: Vec<_> = commands
                .iter()
                .filter_map(|command| match command {
                    TeletextEnhancementCommand::InvokeObject {
                        target,
                        object_type,
                        offset,
                    } if *target == identity(owner) => Some((*object_type, *offset)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                calls,
                [32, 34].map(|column| (
                    TeletextObjectType::Passive,
                    Some(TeletextObjectOffset::new(0, column).unwrap())
                ))
            );
        }
        assert_pixels(service, 0, index == 1);
    }
    assert_eq!(
        std::array::from_fn::<_, 4, _>(|target| word(0, target)),
        [0x20348, 0x00349, 0x20340, 0x00341]
    );
    assert_eq!(
        std::array::from_fn::<_, 4, _>(|target| word(1, target)),
        [0x20b48, 0x00b49, 0x20b40, 0x00b41]
    );
}
