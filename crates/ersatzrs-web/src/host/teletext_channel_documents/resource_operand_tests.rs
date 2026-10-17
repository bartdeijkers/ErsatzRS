//! Complete resource-scalar replay; no receipt, installed-profile or client proof.
use super::*;
use crate::host::teletext_channel_documents::{
    encode_original_channel_documents_with_editorial_programs,
    encode_original_channel_documents_with_editorial_resources,
    encode_original_producer_documents_with_editorial_resources, EditorialResourceDocuments,
};
use ersatzrs_ffmpeg::teletext::{
    lift_teletext_v1_page_set, TeletextResourceFullUpdate, TeletextServiceResourceTransmission,
};

fn raw(page: u8) -> TeletextRawDataPageNumber {
    TeletextRawDataPageNumber::new(2, page).unwrap()
}
fn resource(kind: TeletextResourceKind, page: u8, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        raw(page),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}
fn identity(global: bool, subpage: u8) -> TeletextObjectIdentity {
    let resource = resource(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        if global { 0xa1 } else { 0xa0 },
        subpage,
    );
    if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(19))
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(19))
    }
    .unwrap()
}
fn fg(colour: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(colour).unwrap(),
    }
}
fn glyph(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn drcs(slot: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Drcs {
        reference: TeletextDrcsGlyphReference {
            table: TeletextDrcsTableScope::Normal,
            subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
            slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        },
    }
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn policy() -> TeletextChannelDocumentEncodingPolicy {
    let mut policy = bounds(1).0;
    policy.transmissions = [
        resource(TeletextResourceKind::Pop, 0xa0, 1),
        resource(TeletextResourceKind::Gpop, 0xa1, 2),
        resource(TeletextResourceKind::Drcs, 0xb0, 0),
    ]
    .map(|resource| TeletextServiceResourceTransmission {
        resource,
        counter: TeletextResourceUpdateCounter::new(1).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    })
    .to_vec();
    policy
}
fn link(page: u8, mask: u16) -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: raw(page) },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(mask),
    )
    .unwrap()
}
fn source(colour: u8) -> TeletextDeclaredService {
    let policy = policy();
    let lifted = lift_teletext_v1_page_set(
        draft(TeletextColour::White, "RESOURCE"),
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let absent = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let mut pages = lifted
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for page in &mut pages {
        page.resources.objects = TeletextResolvedObjectPageLinks {
            public: Some(raw(0xa0)),
            global: Some(raw(0xa1)),
        };
        page.resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xb0)), None).unwrap();
        page.compositional_links = Some(
            TeletextX27Level25Links::new(link(0xa1, 4), link(0xa0, 2), absent, link(0xb0, 1))
                .unwrap(),
        );
        page.local_program = Some(
            TeletextEnhancementProgram::new(
                [false, true]
                    .map(|global| TeletextEnhancementCommand::InvokeObject {
                        target: identity(global, if global { 2 } else { 1 }),
                        object_type: TeletextObjectType::Passive,
                        offset: None,
                    })
                    .to_vec(),
            )
            .unwrap(),
        );
    }
    let objects = [false, true]
        .map(|global| TeletextEnhancementObjectDefinition {
            identity: identity(global, if global { 2 } else { 1 }),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(if global { 4 } else { 3 }, 4)
                        .unwrap(),
                },
                at(4, fg(colour)),
                at(4, glyph(b'A')),
                at(5, drcs(0)),
            ])
            .unwrap(),
        })
        .to_vec();
    let service = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages,
            magazines: vec![],
            objects,
            drcs: vec![TeletextDrcsSubtable::new(
                resource(TeletextResourceKind::Drcs, 0xb0, 0),
                vec![TeletextDrcsGlyphDefinition {
                    slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                    bitmap: TeletextDrcsBitmap::new([1; 10]).unwrap(),
                }],
            )
            .unwrap()],
        },
        policy.validation.service,
    )
    .unwrap();
    TeletextDeclaredService::new(
        service,
        vec![],
        [123, 124]
            .map(|page| TeletextDisplayMotSelection {
                display: address(page),
                mot: None,
            })
            .to_vec(),
        vec![],
        policy.validation.declarations,
    )
    .unwrap()
}
fn edits(
    resource_operands: &[TeletextResourceObjectOperandOverlay],
) -> EditorialResourceDocuments<'_> {
    EditorialResourceDocuments {
        programs: (vec![], &[], &[], &[], &[]),
        resource_operands,
        resource_programs: &[],
    }
}
fn replay(
    selected: &TeletextChannelSelectionSnapshot,
    source: TeletextDeclaredService,
    changes: &[TeletextResourceObjectOperandOverlay],
    policy: TeletextChannelDocumentEncodingPolicy,
) -> Result<EncodedChannelTeletextOriginals, ChannelTeletextDocumentEncodingError> {
    encode_original_channel_documents_with_editorial_resources(
        selected,
        Some(ADDON),
        vec![TeletextProfileDocument::Authored(Box::new(source))],
        edits(changes),
        deadline(),
        None,
        policy,
    )
}
fn units(original: &EncodedChannelTeletextOriginals) -> Vec<Vec<[u8; 46]>> {
    let encoded = original.encoded.as_ref().unwrap();
    std::iter::once(encoded.magazine_units().to_vec())
        .chain(
            encoded
                .pages()
                .iter()
                .map(|page| page.data_units().to_vec()),
        )
        .collect()
}
fn pixels(
    original: &EncodedChannelTeletextOriginals,
    page: u16,
) -> ersatzrs_ffmpeg::teletext::TeletextRaster {
    project_teletext_preview(
        original.encoded.as_ref().unwrap(),
        address(page),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        bounds(1).1,
    )
    .unwrap()
    .raster()
    .clone()
}

#[test]
fn resource_replay_changes_pop_gpop_bytes_and_both_previews_without_rewriting_original_profiles() {
    let day = snapshot(1, 11);
    let night = snapshot(2, 12);
    assert_eq!(day.bindings[0].addon_id, night.bindings[0].addon_id);
    assert_ne!(day.bindings[0].profile_id, night.bindings[0].profile_id);
    let root = source(1);
    let original = replay(&day, root.clone(), &[], policy()).unwrap();
    let other = replay(&night, source(4), &[], policy()).unwrap();
    let changes = [
        TeletextResourceObjectOperandOverlay::new(identity(false, 1), 1, 4, fg(1), fg(2)).unwrap(),
        TeletextResourceObjectOperandOverlay::new(
            identity(true, 2),
            2,
            4,
            glyph(b'A'),
            glyph(b'B'),
        )
        .unwrap(),
    ];
    let candidate = replay(&day, root.clone(), &changes, policy()).unwrap();
    assert_eq!(candidate.producers[0].original(), &root);
    assert_eq!(candidate.provenance, original.provenance);
    assert_eq!(candidate.metadata, original.metadata);
    assert_eq!(candidate.transmissions, original.transmissions);
    assert_ne!(units(&candidate), units(&original));
    for page in [123, 124] {
        assert_ne!(pixels(&candidate, page), pixels(&original, page));
    }
    assert_eq!(
        units(&other),
        units(&replay(&night, source(4), &[], policy()).unwrap())
    );
    assert_ne!(pixels(&other, 123), pixels(&original, 123));
    let encoded = candidate.encoded.as_ref().unwrap();
    assert_eq!(
        encoded
            .declared()
            .semantic()
            .objects()
            .definition(identity(false, 1))
            .unwrap()
            .program
            .commands()[1],
        at(4, fg(2))
    );
    assert_eq!(
        encoded
            .declared()
            .semantic()
            .objects()
            .definition(identity(true, 2))
            .unwrap()
            .program
            .commands()[2],
        at(4, glyph(b'B'))
    );
}

#[test]
fn resource_replay_empty_extension_keeps_legacy_encoding_and_absence_exact() {
    let selected = snapshot(1, 11);
    let root = source(1);
    let original = replay(&selected, root.clone(), &[], policy()).unwrap();
    let legacy = encode_original_channel_documents_with_editorial_programs(
        &selected,
        Some(ADDON),
        vec![TeletextProfileDocument::Authored(Box::new(root))],
        (vec![], &[], &[], &[], &[]),
        deadline(),
        None,
        policy(),
    )
    .unwrap();
    assert_eq!(units(&original), units(&legacy));
    assert_eq!(original.descriptor, legacy.descriptor);
    let absent = encode_original_producer_documents_with_editorial_resources(
        None,
        vec![],
        edits(&[]),
        deadline(),
        None,
        policy(),
    )
    .unwrap();
    assert!(absent.encoded.is_none());
    let changes =
        [
            TeletextResourceObjectOperandOverlay::new(identity(false, 1), 1, 4, fg(1), fg(2))
                .unwrap(),
        ];
    assert!(matches!(
        encode_original_producer_documents_with_editorial_resources(
            None,
            vec![],
            edits(&changes),
            deadline(),
            None,
            policy()
        ),
        Err(ChannelTeletextDocumentEncodingError::Association)
    ));
}

#[test]
fn resource_replay_rejects_stale_full_identity_and_unresolved_dependency_without_output() {
    let selected = snapshot(1, 11);
    let changes = [
        TeletextResourceObjectOperandOverlay::new(identity(false, 0), 1, 4, fg(1), fg(2)).unwrap(),
        TeletextResourceObjectOperandOverlay::new(identity(true, 1), 1, 4, fg(1), fg(2)).unwrap(),
        TeletextResourceObjectOperandOverlay::new(identity(false, 1), 1, 4, fg(3), fg(2)).unwrap(),
        TeletextResourceObjectOperandOverlay::new(identity(false, 1), 3, 5, drcs(0), drcs(1))
            .unwrap(),
    ];
    for change in changes {
        assert!(matches!(
            replay(&selected, source(1), &[change], policy()),
            Err(ChannelTeletextDocumentEncodingError::Composition(_))
        ));
    }
}

#[test]
fn resource_replay_bounds_combined_raw_operands_before_producer_allocation() {
    let mut policy = policy();
    policy.composition.service.max_commands = 1;
    // No producer storage is admitted. The two-edit aggregate must reject first.
    policy.composition.max_storage_bytes = 0;
    let resource =
        [
            TeletextResourceObjectOperandOverlay::new(identity(false, 1), 1, 4, fg(1), fg(2))
                .unwrap(),
        ];
    let local =
        [TeletextLocalProgramOperandOverlay::new(address(123), 0, 0, fg(1), fg(2)).unwrap()];
    let error = encode_original_channel_documents_with_editorial_resources(
        &snapshot(1, 11),
        Some(ADDON),
        vec![TeletextProfileDocument::Authored(Box::new(source(1)))],
        EditorialResourceDocuments {
            programs: (vec![], &local, &[], &[], &[]),
            resource_operands: &resource,
            resource_programs: &[],
        },
        deadline(),
        None,
        policy,
    );
    assert!(
        matches!(error,Err(ChannelTeletextDocumentEncodingError::Composition(error))
        if matches!(&error.reason,TeletextProducerCompositionFailure::Semantic(reason)
            if **reason==TeletextServiceError::Budget {dimension:"authored command",maximum:1}))
    );
}
