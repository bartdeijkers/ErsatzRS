//! Exact bitmap intent and immutable semantic application, not editor authority.
use super::*;

fn bitmap(first: u16) -> TeletextDrcsBitmap {
    let mut rows = [0; 10];
    rows[0] = first;
    TeletextDrcsBitmap::new(rows).unwrap()
}

fn slot(value: u8) -> TeletextDrcsGlyphSlot {
    TeletextDrcsGlyphSlot::new(value).unwrap()
}

fn change(
    identity: TeletextResourceIdentity,
    glyph: u8,
    expected: u16,
    replacement: u16,
) -> TeletextDrcsBitmapOverlay {
    TeletextDrcsBitmapOverlay::new(identity, slot(glyph), bitmap(expected), bitmap(replacement))
        .unwrap()
}

fn resource_table(identity: TeletextResourceIdentity) -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(
        identity,
        vec![
            TeletextDrcsGlyphDefinition {
                slot: slot(7),
                bitmap: bitmap(0x456),
            },
            TeletextDrcsGlyphDefinition {
                slot: slot(0),
                bitmap: bitmap(0x801),
            },
        ],
    )
    .unwrap()
}

#[test]
fn drcs_bitmap_intent_rejects_wrong_resource_kinds_and_noop() {
    for kind in [
        TeletextResourceKind::Pop,
        TeletextResourceKind::Gpop,
        TeletextResourceKind::Mot,
        TeletextResourceKind::Mip,
    ] {
        let page = match kind {
            TeletextResourceKind::Mot => 0xfe,
            TeletextResourceKind::Mip => 0xfd,
            _ => 0xa0,
        };
        assert_eq!(
            TeletextDrcsBitmapOverlay::new(resource(kind, page, 0), slot(0), bitmap(1), bitmap(3)),
            Err(TeletextDrcsBitmapOverlayError::Kind)
        );
    }
    let identity = resource(TeletextResourceKind::Drcs, 0xc0, 1);
    assert_eq!(
        TeletextDrcsBitmapOverlay::new(identity, slot(0), bitmap(0x801), bitmap(0x801)),
        Err(TeletextDrcsBitmapOverlayError::Unchanged)
    );
}

#[test]
fn drcs_bitmaps_change_exact_normal_and_global_resources_without_reordering() {
    let normal = resource(TeletextResourceKind::Drcs, 0xc0, 1);
    let global = resource(TeletextResourceKind::Gdrcs, 0xc1, 15);
    let sibling = resource(TeletextResourceKind::Drcs, 0xc0, 0);
    let mut input = draft(&[address(200, 0), address(200, 1)]);
    input.drcs = vec![
        resource_table(global),
        resource_table(sibling),
        resource_table(normal),
    ];
    let original = TeletextSemanticService::new(input.clone(), budget()).unwrap();
    let before = original.clone();
    let overlays = [
        change(normal, 0, 0x801, 0x803),
        change(global, 7, 0x456, 0x457),
    ];
    let changed = original
        .clone()
        .with_drcs_bitmap_overlays(&overlays, budget())
        .unwrap();
    input.drcs[0] = TeletextDrcsSubtable::new(
        global,
        vec![
            TeletextDrcsGlyphDefinition {
                slot: slot(7),
                bitmap: bitmap(0x457),
            },
            TeletextDrcsGlyphDefinition {
                slot: slot(0),
                bitmap: bitmap(0x801),
            },
        ],
    )
    .unwrap();
    input.drcs[2] = TeletextDrcsSubtable::new(
        normal,
        vec![
            TeletextDrcsGlyphDefinition {
                slot: slot(7),
                bitmap: bitmap(0x456),
            },
            TeletextDrcsGlyphDefinition {
                slot: slot(0),
                bitmap: bitmap(0x803),
            },
        ],
    )
    .unwrap();
    assert_eq!(
        changed,
        TeletextSemanticService::new(input, budget()).unwrap()
    );
    assert_eq!(original, before);
    assert_ne!(changed, original);
    assert_eq!(overlays[0].identity(), normal);
    assert_eq!(overlays[0].slot(), slot(0));
    assert_eq!(overlays[0].expected().get()[0], 0x801);
    assert_eq!(overlays[0].replacement().get()[0], 0x803);
}

#[test]
fn drcs_bitmaps_reject_empty_duplicate_missing_undefined_and_stale_targets() {
    let identity = resource(TeletextResourceKind::Drcs, 0xc0, 1);
    let mut input = draft(&[address(200, 0)]);
    input.drcs = vec![resource_table(identity)];
    let original = TeletextSemanticService::new(input, budget()).unwrap();
    let valid = change(identity, 0, 0x801, 0x803);
    assert_eq!(
        original.clone().with_drcs_bitmap_overlays(&[], budget()),
        Err(TeletextServiceError::EmptyDrcsBitmapOverlays)
    );
    for (overlays, reason) in [
        (vec![valid, valid], TeletextDrcsBitmapFailure::Duplicate),
        (
            vec![change(identity, 1, 0x801, 0x803)],
            TeletextDrcsBitmapFailure::MissingGlyph,
        ),
        (
            vec![change(identity, 0, 0x800, 0x803)],
            TeletextDrcsBitmapFailure::BitmapMismatch,
        ),
        (
            vec![change(
                resource(TeletextResourceKind::Drcs, 0xc0, 0),
                0,
                0x801,
                0x803,
            )],
            TeletextDrcsBitmapFailure::MissingResource,
        ),
        (
            vec![change(
                resource(TeletextResourceKind::Gdrcs, 0xc0, 1),
                0,
                0x801,
                0x803,
            )],
            TeletextDrcsBitmapFailure::MissingResource,
        ),
    ] {
        let first = overlays[0];
        assert_eq!(
            original
                .clone()
                .with_drcs_bitmap_overlays(&overlays, budget()),
            Err(TeletextServiceError::DrcsBitmapOverlay {
                identity: first.identity(),
                slot: first.slot(),
                reason
            })
        );
    }
    let changed = original
        .clone()
        .with_drcs_bitmap_overlays(&[valid], budget())
        .unwrap();
    assert!(matches!(
        changed.with_drcs_bitmap_overlays(&[valid], budget()),
        Err(TeletextServiceError::DrcsBitmapOverlay {
            reason: TeletextDrcsBitmapFailure::BitmapMismatch,
            ..
        })
    ));
    assert_eq!(
        original.drcs().subtables()[0].glyphs()[1].bitmap,
        bitmap(0x801)
    );
}

fn rich_service(input: TeletextResolvedServiceDraft) -> TeletextSemanticService {
    let owner = address(200, 0);
    let companion = resource(TeletextResourceKind::Pop, 0xa0, 0);
    let mut semantic =
        TeletextSemanticService::new_with_object_companions(input, vec![companion], budget())
            .unwrap();
    semantic.set_mot_fallback(address(200, 1), presentation(0, 3));
    semantic
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner,
                vec![TeletextPhysicalBaseRow::new(6, [b'P'; 40]).unwrap()],
            )
            .unwrap()],
            budget(),
        )
        .unwrap()
}

fn rich_input() -> TeletextResolvedServiceDraft {
    let mut input = draft(&[address(200, 0), address(200, 1)]);
    let owner = &mut input.pages[0];
    owner.presentation = Some(presentation(0x37, 1));
    owner.resources.objects.public = Some(raw(0xa0));
    owner.resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xc0)), None).unwrap();
    owner.local_program = Some(program(vec![
        TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(1).unwrap(),
            },
        },
        drcs(0),
    ]));
    owner.resources.defaults[2] = Some(TeletextDefaultObjectSelection {
        target: public(0xa0, 1, 1),
        object_type: TeletextObjectType::Passive,
    });
    input.objects.push(definition(
        public(0xa0, 1, 1),
        TeletextObjectType::Passive,
        vec![],
    ));
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 3,
        presentation: presentation(0x20, 2),
    });
    input.drcs = vec![resource_table(resource(
        TeletextResourceKind::Drcs,
        0xc0,
        0,
    ))];
    input
}

#[test]
fn drcs_bitmaps_preserve_local_changes_physical_rows_mot_fallbacks_defaults_and_companions() {
    let mut input = rich_input();
    let local = TeletextLocalProgramOperandOverlay::new(
        address(200, 0),
        0,
        0,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(1).unwrap(),
        },
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(2).unwrap(),
        },
    )
    .unwrap();
    let original = rich_service(input.clone())
        .with_local_program_operand_overlays(&[local], budget())
        .unwrap();
    let before = original.clone();
    let changed = original
        .clone()
        .with_drcs_bitmap_overlays(
            &[change(
                resource(TeletextResourceKind::Drcs, 0xc0, 0),
                0,
                0x801,
                0x803,
            )],
            budget(),
        )
        .unwrap();
    let mut glyphs = input.drcs[0].glyphs().to_vec();
    glyphs[1].bitmap = bitmap(0x803);
    input.drcs[0] = TeletextDrcsSubtable::new(input.drcs[0].identity(), glyphs).unwrap();
    let expected = rich_service(input)
        .with_local_program_operand_overlays(&[local], budget())
        .unwrap();
    assert_eq!(changed, expected);
    assert_eq!(original, before);
    assert_eq!(
        changed.collected(address(200, 0)),
        original.collected(address(200, 0))
    );
}

#[test]
fn drcs_bitmap_preflight_rejects_existing_and_replacement_work_budget_exhaustion() {
    let original = rich_service(rich_input());
    let valid = change(
        resource(TeletextResourceKind::Drcs, 0xc0, 0),
        0,
        0x801,
        0x803,
    );
    for dimension in [
        "record",
        "object",
        "structure item",
        "base content byte",
        "authored command",
        "collected command",
        "collected DRCS reference",
    ] {
        let mut limits = budget();
        match dimension {
            "record" => limits.max_records = 2,
            "object" => limits.max_objects = 0,
            "structure item" => limits.max_structure_items = 1,
            "base content byte" => limits.max_base_content_bytes = 39,
            "authored command" => limits.max_commands = 0,
            "collected command" => limits.max_collected_commands = 0,
            _ => limits.max_collected_drcs_references = 0,
        }
        assert!(
            matches!(original.clone().with_drcs_bitmap_overlays(&[valid], limits),
            Err(TeletextServiceError::Budget { dimension: actual, .. }) if actual == dimension),
            "{dimension}"
        );
    }
    let mut limits = budget();
    limits.max_structure_items = 1;
    assert_eq!(
        original.with_drcs_bitmap_overlays(&[valid, valid], limits),
        Err(TeletextServiceError::Budget {
            dimension: "structure item",
            maximum: 1
        })
    );
}
