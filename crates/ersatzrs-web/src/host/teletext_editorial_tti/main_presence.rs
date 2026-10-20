//! An absent Main may acquire a standalone authored X26 program, never an inferred one.
use super::{
    operands, Location, NativeTeletextEditorialTtiError, NativeTeletextEditorialTtiFailure, Page,
    Result, TeletextEditorialChanges,
};
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCharsetDesignation,
    TeletextEnhancementCommand as Command, TeletextEnhancementPosition, TeletextEnhancementProgram,
    TeletextFlashMode, TeletextFlashRate, TeletextMainProgramPresenceOverlay, TeletextPageAddress,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use std::time::Instant;

const FINAL: u32 = 63 | (31 << 6) | (7 << 11);

fn fail(location: Location, word: usize) -> NativeTeletextEditorialTtiError {
    NativeTeletextEditorialTtiError {
        row: Some(26),
        designation: u8::try_from(word / 13).ok(),
        column: Some(word % 13),
        ..location.error(NativeTeletextEditorialTtiFailure::Structure)
    }
}

/// Only an actual optional-presence edge uses this role. Existing generated
/// packets and Some-to-Some edits keep their original source-bound reconciliation.
pub(super) fn eligible(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    baseline: &Page<'_>,
    edited: &Page<'_>,
) -> bool {
    let Some(page) = service
        .declared()
        .semantic()
        .page_enhancements()
        .get(&owner)
    else {
        return false;
    };
    let before = baseline.packets[0].iter().any(Option::is_some);
    let after = edited.packets[0].iter().any(Option::is_some);
    if page.local_program.is_none() {
        !before && after
    } else {
        before && !after
    }
}

pub(super) fn reconcile(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    baseline: &Page<'_>,
    edited: &Page<'_>,
    location: Location,
    deadline: Instant,
) -> Result<TeletextEditorialChanges> {
    operands::check_packets(service, owner, baseline, location, deadline)?;
    let original = service
        .declared()
        .semantic()
        .page_enhancements()
        .get(&owner)
        .ok_or_else(|| fail(location, 0))?
        .local_program
        .as_ref();
    let replacement = if original.is_none() {
        // Do not turn compatibility words, default programs or definitions into
        // authored Main intent. Their existing source-bound paths stay separate.
        if baseline.packets[0].iter().any(Option::is_some) {
            return Err(fail(location, 0));
        }
        Some(parse(edited, location, deadline)?)
    } else {
        // Removing every X26 packet means absent, not Some(empty). Native replay
        // must still reproduce every final packet before any concept is saved.
        if edited.packets[0].iter().any(Option::is_some) {
            return Err(fail(location, 0));
        }
        None
    };
    let intent = TeletextMainProgramPresenceOverlay::new(owner, original.cloned(), replacement)
        .map_err(|_| fail(location, 0))?;
    Ok(TeletextEditorialChanges {
        main_presence: vec![intent],
        ..Default::default()
    })
}

fn parse(
    page: &Page<'_>,
    location: Location,
    deadline: Instant,
) -> Result<TeletextEnhancementProgram> {
    let mut words = Vec::new();
    let mut absent = false;
    for (designation, packet) in page.packets[0].iter().enumerate() {
        location.check_deadline(deadline)?;
        match packet {
            Some(packet) if !absent => words.extend_from_slice(*packet),
            None => absent = true,
            _ => return Err(fail(location, designation * 13)),
        }
    }
    let end = words
        .iter()
        .position(|word| *word == FINAL)
        .ok_or_else(|| fail(location, 0))?;
    if words.len() != (end + 1).div_ceil(13) * 13 || words[end..].iter().any(|word| *word != FINAL)
    {
        return Err(fail(location, end));
    }
    let body = super::structural::raw::Body::read(&words[..end], location, deadline)?;
    if let Some(selector) = body.selectors.iter().find(|selector| !selector.used) {
        return Err(fail(location, selector.index));
    }
    let mut commands = Vec::new();
    for entry in body.entries {
        location.check_deadline(deadline)?;
        commands.push(
            entry
                .command
                .or(entry.style)
                .or_else(|| context(entry.word))
                .ok_or_else(|| fail(location, entry.index))?,
        );
    }
    TeletextEnhancementProgram::new(commands).map_err(|_| fail(location, 0))
}

fn context(word: u32) -> Option<Command> {
    if word > 0x3ffff {
        return None;
    }
    let address = u8::try_from(word & 63).ok()?;
    let mode = (word >> 6) & 31;
    let data = u8::try_from(word >> 11).ok()?;
    if address >= 40 {
        let row = if address == 40 { 24 } else { address - 40 };
        return match mode {
            0 => Some(Command::SetScreenColour {
                colour: TeletextColourReference::new(data).ok()?,
            }),
            4 => Some(Command::SetPosition {
                position: TeletextEnhancementPosition::new(row, data).ok()?,
            }),
            1 | 7 if mode == 1 || address == 63 => {
                if data & 0x60 != 0 && data & 0x60 != 0x60 {
                    return None;
                }
                Some(Command::SetRowColour {
                    row: if mode == 7 { 0 } else { row },
                    colour: TeletextColourReference::new(data & 31).ok()?,
                    following_rows: data & 0x60 != 0,
                })
            }
            _ => None,
        };
    }
    let operation = match mode {
        8 => TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(data).ok()?,
        },
        7 if data < 24 => TeletextColumnOperation::Flash {
            mode: match data & 3 {
                0 => TeletextFlashMode::Steady,
                1 => TeletextFlashMode::Normal,
                2 => TeletextFlashMode::Inverted,
                _ => TeletextFlashMode::AdjacentClut,
            },
            rate: match data >> 2 {
                0 => TeletextFlashRate::Slow1Hz,
                1 => TeletextFlashRate::Fast2HzPhase1,
                2 => TeletextFlashRate::Fast2HzPhase2,
                3 => TeletextFlashRate::Fast2HzPhase3,
                4 => TeletextFlashRate::Fast2HzIncremental,
                5 => TeletextFlashRate::Fast2HzDecremental,
                _ => return None,
            },
        },
        _ => return None,
    };
    Some(Command::AtColumn {
        column: address,
        operation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn read(words: &[u32]) -> Result<TeletextEnhancementProgram> {
        let mut words = words.to_vec();
        words.push(FINAL);
        words.resize(words.len().div_ceil(13) * 13, FINAL);
        let packets: Vec<[u32; 13]> = words
            .chunks_exact(13)
            .map(|p| p.try_into().unwrap())
            .collect();
        let mut page = Page::default();
        for (index, packet) in packets.iter().enumerate() {
            page.packets[0][index] = Some(packet);
        }
        parse(
            &page,
            Location {
                page: 124,
                subcode: 32,
                resource: None,
            },
            Instant::now() + Duration::from_secs(1),
        )
    }

    #[test]
    fn standalone_main_reader_preserves_empty_and_explicit_context() {
        assert!(read(&[]).unwrap().commands().is_empty());
        let words = [
            0x2a | (4 << 6) | (8 << 11),
            8 | (18 << 11),
            8 | (9 << 6) | (81 << 11),
            9 | (7 << 11),
        ];
        let program = read(&words).unwrap();
        assert_eq!(
            program.commands(),
            &[
                Command::SetPosition {
                    position: TeletextEnhancementPosition::new(2, 8).unwrap()
                },
                Command::AtColumn {
                    column: 8,
                    operation: TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(18).unwrap()
                    }
                },
                Command::AtColumn {
                    column: 8,
                    operation: TeletextColumnOperation::G0 {
                        code: ersatzrs_domain::TeletextGlyphCode::new(81).unwrap()
                    }
                },
                Command::AtColumn {
                    column: 9,
                    operation: TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(7).unwrap()
                    }
                },
            ]
        );
        assert!(matches!(
            read(&[8 | (7 << 6) | (23 << 11)]).unwrap().commands(),
            [Command::AtColumn {
                operation: TeletextColumnOperation::Flash {
                    mode: TeletextFlashMode::AdjacentClut,
                    rate: TeletextFlashRate::Fast2HzDecremental
                },
                ..
            }]
        ));
        assert!(
            matches!(read(&[8 | (8<<6) | (55<<11)]).unwrap().commands(), [Command::AtColumn {operation:TeletextColumnOperation::DesignateCharset {designation}, ..}] if designation.get() == 55)
        );
    }

    #[test]
    fn standalone_main_reader_rejects_unknown_reserved_unbound_and_malformed_words() {
        for word in [
            8 | (14 << 6),
            8 | (7 << 6) | (24 << 11),
            8 | (12 << 6) | (8 << 11),
            8 | (13 << 6),
            0x28 | (24 << 6) | (16 << 11),
            63 | (31 << 6) | (5 << 11),
        ] {
            let error = read(&[word]).unwrap_err();
            assert_eq!((error.page, error.subcode, error.row), (124, 32, Some(26)));
            if ![13, 24].contains(&((word >> 6) & 31)) {
                assert_eq!((error.designation, error.column), (Some(0), Some(0)));
            }
        }
        let packet = [FINAL; 13];
        let location = Location {
            page: 124,
            subcode: 32,
            resource: None,
        };
        let mut page = Page::default();
        page.packets[0][1] = Some(&packet);
        assert!(parse(&page, location, Instant::now() + Duration::from_secs(1)).is_err());
        assert_eq!(
            parse(&page, location, Instant::now()).unwrap_err().failure,
            NativeTeletextEditorialTtiFailure::TimedOut
        );
    }
}
