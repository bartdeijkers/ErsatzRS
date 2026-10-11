use super::{d, wire, TeletextAuthoredValueError as Error};

pub(super) fn convert(
    value: &wire::TeletextLevel25PresentationWire,
) -> Result<d::TeletextLevel25PresentationDefaults, Error> {
    Ok(d::TeletextLevel25PresentationDefaults {
        primary: d::TeletextPageCharsetDesignation::new(value.primary)?,
        secondary: value
            .secondary
            .map(d::TeletextPageCharsetDesignation::new)
            .transpose()?,
        palette: d::TeletextLevel25Palette {
            clut2: colours(value.palette.clut2)?,
            clut3: colours(value.palette.clut3)?,
        },
        screen_colour: d::TeletextColourReference::new(value.screen_colour)?,
        row_colour: d::TeletextColourReference::new(value.row_colour)?,
        black_background_substitution: value.black_background_substitution,
        remapping: remapping(value.remapping),
        panels: d::TeletextSidePanels {
            left: value.panels.left,
            right: value.panels.right,
            status: match value.panels.status {
                wire::TeletextPanelStatusWire::Level35Only => d::TeletextPanelStatus::Level35Only,
                wire::TeletextPanelStatusWire::Level25And35 => d::TeletextPanelStatus::Level25And35,
            },
            width: d::TeletextPanelWidth::new(value.panels.width)?,
        },
    })
}

fn colours(value: [[u8; 3]; 8]) -> Result<[d::TeletextRgb; 8], Error> {
    let mut output = [d::TeletextRgb::new([0; 3])?; 8];
    for (destination, source) in output.iter_mut().zip(value) {
        *destination = d::TeletextRgb::new(source)?;
    }
    Ok(output)
}

fn remapping(value: wire::TeletextColourRemappingWire) -> d::TeletextColourRemapping {
    match value {
        wire::TeletextColourRemappingWire::FgClut0BgClut0 => {
            d::TeletextColourRemapping::FgClut0BgClut0
        }
        wire::TeletextColourRemappingWire::FgClut0BgClut1 => {
            d::TeletextColourRemapping::FgClut0BgClut1
        }
        wire::TeletextColourRemappingWire::FgClut0BgClut2 => {
            d::TeletextColourRemapping::FgClut0BgClut2
        }
        wire::TeletextColourRemappingWire::FgClut1BgClut1 => {
            d::TeletextColourRemapping::FgClut1BgClut1
        }
        wire::TeletextColourRemappingWire::FgClut1BgClut2 => {
            d::TeletextColourRemapping::FgClut1BgClut2
        }
        wire::TeletextColourRemappingWire::FgClut2BgClut1 => {
            d::TeletextColourRemapping::FgClut2BgClut1
        }
        wire::TeletextColourRemappingWire::FgClut2BgClut2 => {
            d::TeletextColourRemapping::FgClut2BgClut2
        }
        wire::TeletextColourRemappingWire::FgClut2BgClut3 => {
            d::TeletextColourRemapping::FgClut2BgClut3
        }
    }
}
