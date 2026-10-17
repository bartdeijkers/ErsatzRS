//! Exact immutable bank/selector binding and six literal native cell masks.
use super::fixture::bank_mask;
use super::*;

fn mask(index: u8, target: usize) -> [u16; 10] {
    let TeletextColumnOperation::Drcs { reference } = operation(index, target) else {
        unreachable!()
    };
    bank_mask(
        reference.table,
        reference.subtable.get(),
        reference.slot.get(),
    )
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

pub(super) fn selectors(service: &EncodedTeletextAuthoredService, index: u8) -> [usize; 2] {
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
    std::array::from_fn(|group| {
        let first = program
            .command_positions()
            .iter()
            .find(|position| position.semantic_word() == positions[group * 2])
            .unwrap();
        let second = program
            .command_positions()
            .iter()
            .find(|position| position.semantic_word() == positions[group * 2 + 1])
            .unwrap();
        assert!(first.first_word() < first.semantic_word());
        assert_eq!(
            second.first_word(),
            second.semantic_word(),
            "one original selector serves two glyph operands in the same context"
        );
        let prefix = first.first_word();
        assert_eq!(
            words[prefix / 13].unwrap()[prefix % 13],
            selector_word(index, group),
            "exact original/final selector scope, Level2.5 flag and S1"
        );
        prefix
    })
}
fn assert_banks(service: &EncodedTeletextAuthoredService) {
    assert!(service.compiler_drcs_resources().is_empty());
    assert_eq!(service.declared().semantic().drcs().subtables().len(), 4);
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
    let normal_link = service.declared().mots()[0].links().drcs_link(Some(
        ersatzrs_domain::TeletextMotPublicLinkIndex::new(1).unwrap(),
    ));
    assert_eq!(normal_link.extent().maximum().get(), 1);
    assert_eq!(normal_link.extent().required_mask().get(), 3);
    for scope in [
        TeletextDrcsTableScope::Normal,
        TeletextDrcsTableScope::Global,
    ] {
        for subpage in 0..2 {
            let table = service
                .declared()
                .semantic()
                .drcs()
                .subtables()
                .iter()
                .find(|table| table.identity() == bank(scope, subpage))
                .unwrap();
            assert_eq!(table.glyphs().len(), 2);
            for slot in 0..2 {
                assert_eq!(table.glyphs()[usize::from(slot)].slot.get(), slot);
                assert_eq!(
                    table.glyphs()[usize::from(slot)].bitmap.get(),
                    bank_mask(scope, subpage, slot)
                );
            }
        }
        for metadata in service.declared().semantic().page_enhancements().values() {
            assert_eq!(
                metadata.resources.drcs.page(scope),
                Some(bank(scope, 0).page())
            );
            let role = if scope == TeletextDrcsTableScope::Normal {
                ersatzrs_domain::TeletextX27LinkRole::Drcs
            } else {
                ersatzrs_domain::TeletextX27LinkRole::Gdrcs
            };
            let link = metadata.compositional_links.unwrap().link(role);
            assert_eq!(link.required_subpages().get(), 3);
            assert_eq!(
                link.target(),
                ersatzrs_domain::TeletextX27LinkTarget::Page {
                    page: bank(scope, 0).page()
                }
            );
        }
    }
}

fn assert_bindings(service: &EncodedTeletextAuthoredService, index: u8) {
    assert_banks(service);
    selectors(service, index);
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
        assert_eq!(
            executed.resource(),
            bank(reference.table, reference.subtable.get())
        );
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
    assert_eq!(changed, [true; 6]);
    assert_fallback_unchanged(before, after);
    assert_pixels(after, index, false);
}
pub(super) fn assert_fallback_unchanged(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
) {
    let [mut original, mut edited] = [before, after].map(|service| {
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
        project_encoded_teletext_display(page, service.unit_count()).unwrap()
    });
    for projection in [&mut original, &mut edited] {
        assert!(projection
            .records
            .iter()
            .any(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. })));
        projection.records.retain(|record| {
            !matches!(
                record,
                TeletextEncodedDisplayRecord::Triplets { row: 26, .. }
            )
        });
    }
    assert_eq!(
        original, edited,
        "all header/fallback and non-X26 records stay original"
    );
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
        [0x20348, 0x20b49, 0x00340, 0x00b41]
    );
    assert_eq!(
        std::array::from_fn::<_, 4, _>(|target| word(1, target)),
        [0x20348, 0x20b49, 0x00340, 0x00b41]
    );
}
