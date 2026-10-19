//! Closed V24 native spelling fields; no raw program or original Local identity.
use super::{invalid, map_only, TeletextDesignDocumentError};
use ersatzrs_domain::{
    TeletextBaseTextSpelling, TeletextColumnOperation, TeletextDiacriticIndex,
    TeletextEnhancementCharsetDesignation, TeletextEnhancementPosition, TeletextGlyphCode,
    TeletextPageAddress,
};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Serialize)]
pub(super) struct StoredNativeSpelling {
    fallback: u8,
    charset: u8,
    mode: u8,
    code: u8,
}
impl<'de> Deserialize<'de> for StoredNativeSpelling {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            fallback: u8,
            charset: u8,
            mode: u8,
            code: u8,
        }
        let value: Fields = map_only(deserializer)?;
        Ok(Self {
            fallback: value.fallback,
            charset: value.charset,
            mode: value.mode,
            code: value.code,
        })
    }
}
#[derive(Default)]
pub(super) struct OptionalNativeSpelling(pub(super) Option<StoredNativeSpelling>);
impl<'de> Deserialize<'de> for OptionalNativeSpelling {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        StoredNativeSpelling::deserialize(deserializer).map(|value| Self(Some(value)))
    }
}
impl StoredNativeSpelling {
    pub(super) fn capture(
        value: TeletextBaseTextSpelling,
    ) -> Result<Self, TeletextDesignDocumentError> {
        let (mode, code) = match value.operation() {
            TeletextColumnOperation::G2 { code } => (0x0f, code.get()),
            TeletextColumnOperation::G0Diacritic { code, diacritic } => {
                (0x10 | diacritic.get(), code.get())
            }
            _ => return Err(invalid()),
        };
        Ok(Self {
            fallback: value.fallback().get(),
            charset: value.charset().get(),
            mode,
            code,
        })
    }
    pub(super) fn restore(
        self,
        owner: TeletextPageAddress,
        position: TeletextEnhancementPosition,
        character: char,
    ) -> Result<TeletextBaseTextSpelling, TeletextDesignDocumentError> {
        let code = TeletextGlyphCode::new(self.code).map_err(|_| invalid())?;
        let operation = match self.mode {
            0x0f => TeletextColumnOperation::G2 { code },
            0x10..=0x1f => TeletextColumnOperation::G0Diacritic {
                code,
                diacritic: TeletextDiacriticIndex::new(self.mode & 15).map_err(|_| invalid())?,
            },
            _ => return Err(invalid()),
        };
        TeletextBaseTextSpelling::new(
            owner,
            position,
            character,
            TeletextGlyphCode::new(self.fallback).map_err(|_| invalid())?,
            TeletextEnhancementCharsetDesignation::new(self.charset).map_err(|_| invalid())?,
            operation,
        )
        .map_err(|_| invalid())
    }
}
