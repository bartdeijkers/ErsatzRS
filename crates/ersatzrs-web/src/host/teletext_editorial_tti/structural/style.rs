//! Checked style words are structural only when their original topology changes.
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCommand,
};

pub(super) fn kind(command: &TeletextEnhancementCommand) -> Option<u8> {
    let TeletextEnhancementCommand::AtColumn { operation, .. } = command else {
        return None;
    };
    match operation {
        TeletextColumnOperation::Foreground { .. } => Some(0),
        TeletextColumnOperation::Background { .. } => Some(3),
        TeletextColumnOperation::Attributes { .. } => Some(12),
        _ => None,
    }
}

pub(super) fn decode(word: u32) -> Option<TeletextEnhancementCommand> {
    if word > 0x3ffff || word & 63 >= 40 {
        return None;
    }
    let column = u8::try_from(word & 63).ok()?;
    let data = u8::try_from(word >> 11).ok()?;
    let operation = match (word >> 6) & 31 {
        0 => TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(data).ok()?,
        },
        3 => TeletextColumnOperation::Background {
            colour: TeletextColourReference::new(data).ok()?,
        },
        12 if data & 8 == 0 => TeletextColumnOperation::Attributes {
            double_height: data & 1 != 0,
            boxed: data & 2 != 0,
            conceal: data & 4 != 0,
            invert: data & 16 != 0,
            underline_or_separated_mosaics: data & 32 != 0,
            double_width: data & 64 != 0,
        },
        _ => return None,
    };
    Some(TeletextEnhancementCommand::AtColumn { column, operation })
}

#[cfg(test)]
mod tests;
