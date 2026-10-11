use super::{
    d, located, resources, wire, TeletextAuthoredLocation as Location, TeletextAuthoredPagesError,
    TeletextAuthoredSection as Section, TeletextAuthoredValueError as Error,
};

pub(super) fn definition(
    value: wire::TeletextEnhancementObjectDefinitionWire,
    index: usize,
) -> Result<d::TeletextEnhancementObjectDefinition, TeletextAuthoredPagesError> {
    let location = Location::new(Section::Objects, index);
    Ok(d::TeletextEnhancementObjectDefinition {
        identity: located(location, resources::object_identity(value.identity))?,
        object_type: object_type(value.object_type),
        program: convert(value.program, location)?,
    })
}

pub(super) fn convert(
    value: wire::TeletextEnhancementProgramWire,
    location: Location,
) -> Result<d::TeletextEnhancementProgram, TeletextAuthoredPagesError> {
    let commands = value
        .commands
        .into_iter()
        .enumerate()
        .map(|(ordinal, value)| {
            located(
                Location {
                    command: Some(ordinal),
                    ..location
                },
                command(value),
            )
        })
        .collect::<Result<_, _>>()?;
    d::TeletextEnhancementProgram::new(commands).map_err(|reason| {
        let ordinal = match reason {
            d::TeletextEnhancementProgramError::Row { ordinal, .. }
            | d::TeletextEnhancementProgramError::Column { ordinal, .. } => ordinal,
        };
        TeletextAuthoredPagesError::Conversion {
            location: Location {
                command: Some(ordinal),
                ..location
            },
            reason: reason.into(),
        }
    })
}

pub(super) fn object_type(value: wire::TeletextObjectTypeWire) -> d::TeletextObjectType {
    match value {
        wire::TeletextObjectTypeWire::Active => d::TeletextObjectType::Active,
        wire::TeletextObjectTypeWire::Adaptive => d::TeletextObjectType::Adaptive,
        wire::TeletextObjectTypeWire::Passive => d::TeletextObjectType::Passive,
    }
}

fn command(
    value: wire::TeletextEnhancementCommandWire,
) -> Result<d::TeletextEnhancementCommand, Error> {
    Ok(match value {
        wire::TeletextEnhancementCommandWire::SetPosition { position } => {
            d::TeletextEnhancementCommand::SetPosition {
                position: d::TeletextEnhancementPosition::new(position.row, position.column)?,
            }
        }
        wire::TeletextEnhancementCommandWire::SetScreenColour { colour } => {
            d::TeletextEnhancementCommand::SetScreenColour {
                colour: d::TeletextColourReference::new(colour)?,
            }
        }
        wire::TeletextEnhancementCommandWire::SetRowColour {
            row,
            colour,
            following_rows,
        } => d::TeletextEnhancementCommand::SetRowColour {
            row,
            colour: d::TeletextColourReference::new(colour)?,
            following_rows,
        },
        wire::TeletextEnhancementCommandWire::AtColumn { column, operation } => {
            d::TeletextEnhancementCommand::AtColumn {
                column,
                operation: operation_value(operation)?,
            }
        }
        wire::TeletextEnhancementCommandWire::InvokeObject {
            target,
            object_type: kind,
            offset,
        } => d::TeletextEnhancementCommand::InvokeObject {
            target: resources::object_identity(target)?,
            object_type: object_type(kind),
            offset: offset
                .map(|value| d::TeletextObjectOffset::new(value.row, value.column))
                .transpose()?,
        },
    })
}

fn operation_value(
    value: wire::TeletextColumnOperationWire,
) -> Result<d::TeletextColumnOperation, Error> {
    Ok(match value {
        wire::TeletextColumnOperationWire::Foreground { colour } => {
            d::TeletextColumnOperation::Foreground {
                colour: d::TeletextColourReference::new(colour)?,
            }
        }
        wire::TeletextColumnOperationWire::Background { colour } => {
            d::TeletextColumnOperation::Background {
                colour: d::TeletextColourReference::new(colour)?,
            }
        }
        wire::TeletextColumnOperationWire::DesignateCharset { designation } => {
            d::TeletextColumnOperation::DesignateCharset {
                designation: d::TeletextEnhancementCharsetDesignation::new(designation)?,
            }
        }
        wire::TeletextColumnOperationWire::G0 { code } => d::TeletextColumnOperation::G0 {
            code: d::TeletextGlyphCode::new(code)?,
        },
        wire::TeletextColumnOperationWire::G1 { mosaic } => d::TeletextColumnOperation::G1 {
            mosaic: d::TeletextMosaic::new(mosaic)?,
        },
        wire::TeletextColumnOperationWire::G1Alpha { code } => {
            d::TeletextColumnOperation::G1Alpha {
                code: d::TeletextG1AlphaCode::new(code)?,
            }
        }
        wire::TeletextColumnOperationWire::G2 { code } => d::TeletextColumnOperation::G2 {
            code: d::TeletextGlyphCode::new(code)?,
        },
        wire::TeletextColumnOperationWire::G3 {
            code,
            compatibility,
        } => d::TeletextColumnOperation::G3 {
            code: d::TeletextGlyphCode::new(code)?,
            compatibility: match compatibility {
                wire::TeletextG3CompatibilityWire::Level15 => d::TeletextG3Compatibility::Level15,
                wire::TeletextG3CompatibilityWire::Level25 => d::TeletextG3Compatibility::Level25,
            },
        },
        wire::TeletextColumnOperationWire::G0Diacritic { code, diacritic } => {
            d::TeletextColumnOperation::G0Diacritic {
                code: d::TeletextGlyphCode::new(code)?,
                diacritic: d::TeletextDiacriticIndex::new(diacritic)?,
            }
        }
        wire::TeletextColumnOperationWire::Drcs { reference } => d::TeletextColumnOperation::Drcs {
            reference: resources::reference(&reference)?,
        },
        wire::TeletextColumnOperationWire::Attributes {
            double_height,
            double_width,
            boxed,
            conceal,
            invert,
            underline_or_separated_mosaics,
        } => d::TeletextColumnOperation::Attributes {
            double_height,
            double_width,
            boxed,
            conceal,
            invert,
            underline_or_separated_mosaics,
        },
        wire::TeletextColumnOperationWire::Flash { mode, rate } => {
            d::TeletextColumnOperation::Flash {
                mode: flash_mode(mode),
                rate: flash_rate(rate),
            }
        }
    })
}

fn flash_mode(value: wire::TeletextFlashModeWire) -> d::TeletextFlashMode {
    match value {
        wire::TeletextFlashModeWire::Steady => d::TeletextFlashMode::Steady,
        wire::TeletextFlashModeWire::Normal => d::TeletextFlashMode::Normal,
        wire::TeletextFlashModeWire::Inverted => d::TeletextFlashMode::Inverted,
        wire::TeletextFlashModeWire::AdjacentClut => d::TeletextFlashMode::AdjacentClut,
    }
}

fn flash_rate(value: wire::TeletextFlashRateWire) -> d::TeletextFlashRate {
    match value {
        wire::TeletextFlashRateWire::Slow1Hz => d::TeletextFlashRate::Slow1Hz,
        wire::TeletextFlashRateWire::Fast2HzPhase1 => d::TeletextFlashRate::Fast2HzPhase1,
        wire::TeletextFlashRateWire::Fast2HzPhase2 => d::TeletextFlashRate::Fast2HzPhase2,
        wire::TeletextFlashRateWire::Fast2HzPhase3 => d::TeletextFlashRate::Fast2HzPhase3,
        wire::TeletextFlashRateWire::Fast2HzIncremental => d::TeletextFlashRate::Fast2HzIncremental,
        wire::TeletextFlashRateWire::Fast2HzDecremental => d::TeletextFlashRate::Fast2HzDecremental,
    }
}
