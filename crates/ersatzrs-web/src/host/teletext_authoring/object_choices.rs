//! Complete read-only choices from an already admitted service and original deadline.
use super::{
    check_deadline, TeletextAuthoringError, TeletextObjectCallChoiceObservation,
    TeletextObjectChoicesObservation, TeletextObjectDefinitionChoiceObservation,
    TeletextObjectPageChoiceObservation,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementObjectDefinition,
    TeletextMotResolutionRequirements, TeletextMotRoleResolution, TeletextPageAddress,
    TeletextResourceKind, TeletextX27LinkRole,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use std::time::Instant;

pub(super) fn observe(
    service: &EncodedTeletextAuthoredService,
    deadline: Instant,
) -> Result<TeletextObjectChoicesObservation, TeletextAuthoringError> {
    check_deadline(deadline)?;
    let mut observed = TeletextObjectChoicesObservation::default();
    for resolution in service.declared().resolutions() {
        check_deadline(deadline)?;
        for (role, kind) in [
            (TeletextX27LinkRole::Pop, TeletextResourceKind::Pop),
            (TeletextX27LinkRole::Gpop, TeletextResourceKind::Gpop),
        ] {
            append_role(
                &mut observed,
                resolution.display(),
                kind,
                *resolution.role(role),
            );
        }
    }
    for definition in service.declared().semantic().objects().definitions() {
        append_definition(&mut observed, definition, deadline)?;
    }
    check_deadline(deadline)?;
    Ok(observed)
}

fn append_role(
    observed: &mut TeletextObjectChoicesObservation,
    display: TeletextPageAddress,
    role: TeletextResourceKind,
    resolution: TeletextMotRoleResolution,
) {
    if let TeletextMotRoleResolution::Selected {
        target,
        requirements,
        ..
    } = resolution
    {
        observed.pages.push(TeletextObjectPageChoiceObservation {
            display,
            role,
            target,
            required_subpages: match requirements {
                TeletextMotResolutionRequirements::MotExtent(extent) => extent.required_mask(),
                TeletextMotResolutionRequirements::X27Mask(mask) => mask,
            },
        });
    }
}

fn append_definition(
    observed: &mut TeletextObjectChoicesObservation,
    definition: &TeletextEnhancementObjectDefinition,
    deadline: Instant,
) -> Result<(), TeletextAuthoringError> {
    check_deadline(deadline)?;
    observed
        .definitions
        .push(TeletextObjectDefinitionChoiceObservation {
            identity: definition.identity,
            object_type: definition.object_type,
        });
    if definition.identity.resource().is_some() {
        for (command_index, command) in definition.program.commands().iter().enumerate() {
            check_deadline(deadline)?;
            if let TeletextEnhancementCommand::InvokeObject {
                target,
                object_type,
                offset,
            } = command
            {
                observed.calls.push(TeletextObjectCallChoiceObservation {
                    caller: definition.identity,
                    command_index,
                    target: *target,
                    object_type: *object_type,
                    offset: *offset,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::*;
    use std::time::Duration;

    fn owner() -> TeletextPageAddress {
        TeletextPageAddress {
            page: TeletextPageNumber::new(123).unwrap(),
            subcode: TeletextSubcode::new(16).unwrap(),
        }
    }
    fn resource(kind: TeletextResourceKind, page: u8) -> TeletextResourceIdentity {
        TeletextResourceIdentity::new(
            kind,
            TeletextRawDataPageNumber::new(1, page).unwrap(),
            TeletextResourceSubpageIndex::new(3).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn selected_choices_preserve_extent_masks_sparse_masks_and_raw_page_identity() {
        let mut observed = TeletextObjectChoicesObservation::default();
        for (role, target, requirements, mask) in [
            (
                TeletextResourceKind::Pop,
                resource(TeletextResourceKind::Pop, 0xa0).page(),
                TeletextMotResolutionRequirements::MotExtent(
                    TeletextMotSubpageExtent::new(5).unwrap(),
                ),
                0x3f,
            ),
            (
                TeletextResourceKind::Gpop,
                resource(TeletextResourceKind::Gpop, 0xaf).page(),
                TeletextMotResolutionRequirements::X27Mask(TeletextSubpageRequirementMask::new(
                    0x8004,
                )),
                0x8004,
            ),
        ] {
            let resolution = TeletextMotRoleResolution::Selected {
                target,
                requirements,
                provenance: TeletextMotLinkProvenance::X27 {
                    display: owner(),
                    role: if role == TeletextResourceKind::Pop {
                        TeletextX27LinkRole::Pop
                    } else {
                        TeletextX27LinkRole::Gpop
                    },
                },
                original_mot: None,
            };
            append_role(&mut observed, owner(), role, resolution);
            assert_eq!(
                observed.pages.last(),
                Some(&TeletextObjectPageChoiceObservation {
                    display: owner(),
                    role,
                    target,
                    required_subpages: TeletextSubpageRequirementMask::new(mask)
                })
            );
        }
        append_role(
            &mut observed,
            owner(),
            TeletextResourceKind::Pop,
            TeletextMotRoleResolution::Unselected,
        );
        assert_eq!(observed.pages.len(), 2);
        let bytes = serde_json::to_vec(&observed).unwrap();
        assert_eq!(
            serde_json::from_slice::<TeletextObjectChoicesObservation>(&bytes).unwrap(),
            observed
        );
        assert!(observed.calls.is_empty());
        assert!(observed.definitions.is_empty());
    }
    #[test]
    fn call_choices_keep_actual_resource_call_ordinal_type_offset_and_all_definition_types() {
        let caller = TeletextObjectIdentity::public(
            resource(TeletextResourceKind::Pop, 0xa0),
            TeletextObjectId::new(1),
        )
        .unwrap();
        let target = TeletextObjectIdentity::global(
            resource(TeletextResourceKind::Gpop, 0xaf),
            TeletextObjectId::new(2),
        )
        .unwrap();
        let offset = Some(TeletextObjectOffset::new(3, 7).unwrap());
        let call = TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Passive,
            offset,
        };
        let definitions = [
            TeletextEnhancementObjectDefinition {
                identity: caller,
                object_type: TeletextObjectType::Active,
                program: TeletextEnhancementProgram::new(vec![
                    TeletextEnhancementCommand::SetPosition {
                        position: TeletextEnhancementPosition::new(1, 8).unwrap(),
                    },
                    call,
                ])
                .unwrap(),
            },
            TeletextEnhancementObjectDefinition {
                identity: target,
                object_type: TeletextObjectType::Passive,
                program: TeletextEnhancementProgram::new(vec![]).unwrap(),
            },
            TeletextEnhancementObjectDefinition {
                identity: TeletextObjectIdentity::local(owner(), TeletextObjectId::new(3)),
                object_type: TeletextObjectType::Adaptive,
                program: TeletextEnhancementProgram::new(vec![call]).unwrap(),
            },
        ];
        let mut observed = TeletextObjectChoicesObservation::default();
        for definition in &definitions {
            append_definition(
                &mut observed,
                definition,
                Instant::now() + Duration::from_secs(1),
            )
            .unwrap();
        }
        assert_eq!(
            observed.calls,
            vec![TeletextObjectCallChoiceObservation {
                caller,
                command_index: 1,
                target,
                object_type: TeletextObjectType::Passive,
                offset
            }]
        );
        assert_eq!(
            observed.definitions,
            definitions
                .iter()
                .map(|definition| TeletextObjectDefinitionChoiceObservation {
                    identity: definition.identity,
                    object_type: definition.object_type
                })
                .collect::<Vec<_>>()
        );
        let snapshot = observed.clone();
        assert_eq!(
            append_definition(&mut observed, &definitions[0], Instant::now())
                .unwrap_err()
                .status,
            super::super::TeletextAuthoringStatus::TimedOut
        );
        assert_eq!(observed, snapshot);
        assert!(observed.pages.is_empty());
    }
}
