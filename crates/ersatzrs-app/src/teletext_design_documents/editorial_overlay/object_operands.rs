//! Closed V4 glyph intent tied to exact original local-object commands.

use super::{
    invalid, map_only, unique_original_owner, StoredOperation, TeletextDesignDocumentError,
    TeletextProfileDocument,
};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextLocalObjectOperandOverlay,
    TeletextObjectId, TeletextObjectIdentity, TeletextPageAddress,
};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

pub(super) fn glyph(operation: TeletextColumnOperation) -> bool {
    matches!(
        operation,
        TeletextColumnOperation::G0 { .. } | TeletextColumnOperation::G1 { .. }
    )
}

#[derive(Serialize)]
pub(in super::super) struct StoredObjectOperand {
    identity: StoredLocalIdentity,
    command: usize,
    column: u8,
    expected: StoredOperation,
    replacement: StoredOperation,
}
impl<'de> Deserialize<'de> for StoredObjectOperand {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            identity: StoredLocalIdentity,
            command: usize,
            column: u8,
            expected: StoredOperation,
            replacement: StoredOperation,
        }
        let fields: Fields = map_only(d)?;
        Ok(Self {
            identity: fields.identity,
            command: fields.command,
            column: fields.column,
            expected: fields.expected,
            replacement: fields.replacement,
        })
    }
}
#[derive(Serialize)]
struct StoredLocalIdentity(TeletextObjectIdentity);
impl<'de> Deserialize<'de> for StoredLocalIdentity {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Local {
                owner: super::super::Entry,
                object_id: TeletextObjectId,
            },
        }
        let Fields::Local { owner, object_id } = map_only(d)?;
        Ok(Self(TeletextObjectIdentity::local(
            TeletextPageAddress {
                page: owner.page,
                subcode: owner.subcode,
            },
            object_id,
        )))
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalStoredObjectOperands(pub(super) Option<Vec<StoredObjectOperand>>);
impl OptionalStoredObjectOperands {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredObjectOperands {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<StoredObjectOperand>::deserialize(d).map(|value| Self(Some(value)))
    }
}
pub(super) fn restore(
    operands: Vec<StoredObjectOperand>,
) -> Result<Vec<TeletextLocalObjectOperandOverlay>, TeletextDesignDocumentError> {
    operands
        .into_iter()
        .map(|operand| {
            TeletextLocalObjectOperandOverlay::new(
                operand.identity.0,
                operand.command,
                operand.column,
                operand.expected.0,
                operand.replacement.0,
            )
            .map_err(|_| invalid())
        })
        .collect()
}
pub(super) fn distinct(
    operands: &[TeletextLocalObjectOperandOverlay],
) -> Result<(), TeletextDesignDocumentError> {
    let mut targets = BTreeSet::new();
    for operand in operands {
        if !targets.insert((operand.identity(), operand.command())) {
            return Err(invalid());
        }
    }
    Ok(())
}
pub(super) fn validate_association(
    operands: &[TeletextLocalObjectOperandOverlay],
    documents: &[TeletextProfileDocument],
) -> Result<(), TeletextDesignDocumentError> {
    for operand in operands {
        let TeletextProfileDocument::Authored(service) =
            unique_original_owner(operand.owner(), documents)?
        else {
            return Err(invalid());
        };
        let definition = service
            .semantic()
            .objects()
            .definition(operand.identity())
            .ok_or_else(invalid)?;
        if definition.program.commands().get(operand.command())
            != Some(&TeletextEnhancementCommand::AtColumn {
                column: operand.column(),
                operation: operand.expected(),
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) struct BorrowedObjectOperands<'a>(pub(super) &'a [TeletextLocalObjectOperandOverlay]);
impl Serialize for BorrowedObjectOperands<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for operand in self.0 {
            sequence.serialize_element(&StoredObjectOperand {
                identity: StoredLocalIdentity(operand.identity()),
                command: operand.command(),
                column: operand.column(),
                expected: StoredOperation(operand.expected()),
                replacement: StoredOperation(operand.replacement()),
            })?;
        }
        sequence.end()
    }
}
