//! Closed V14 scalar intent bound to exact immutable POP/GPOP definitions.

use super::{
    invalid, map_only, StoredOperation, TeletextDesignDocumentError, TeletextProfileDocument,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextObjectId, TeletextObjectIdentity,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceObjectOperandOverlay, TeletextResourceSubpageIndex,
};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

#[derive(Serialize)]
pub(in super::super) struct StoredResourceObjectOperand {
    identity: StoredResourceObjectIdentity,
    command: usize,
    column: u8,
    expected: StoredOperation,
    replacement: StoredOperation,
}
impl<'de> Deserialize<'de> for StoredResourceObjectOperand {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            identity: StoredResourceObjectIdentity,
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
pub(super) struct StoredResourceObjectIdentity(pub(super) TeletextObjectIdentity);
impl<'de> Deserialize<'de> for StoredResourceObjectIdentity {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Public {
                resource: StoredResource,
                object_id: TeletextObjectId,
            },
            Global {
                resource: StoredResource,
                object_id: TeletextObjectId,
            },
        }
        let identity = match map_only(d)? {
            Fields::Public {
                resource,
                object_id,
            } => TeletextObjectIdentity::public(resource.0, object_id),
            Fields::Global {
                resource,
                object_id,
            } => TeletextObjectIdentity::global(resource.0, object_id),
        }
        .map_err(serde::de::Error::custom)?;
        Ok(Self(identity))
    }
}
struct StoredResource(TeletextResourceIdentity);
impl<'de> Deserialize<'de> for StoredResource {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            kind: TeletextResourceKind,
            page: StoredRawPage,
            logical_subpage: TeletextResourceSubpageIndex,
        }
        let fields: Fields = map_only(d)?;
        TeletextResourceIdentity::new(fields.kind, fields.page.0, fields.logical_subpage)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}
struct StoredRawPage(TeletextRawDataPageNumber);
impl<'de> Deserialize<'de> for StoredRawPage {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            magazine: u8,
            page: u8,
        }
        let fields: Fields = map_only(d)?;
        TeletextRawDataPageNumber::new(fields.magazine, fields.page)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Default, Serialize)]
pub(super) struct OptionalStoredResourceObjectOperands(
    pub(super) Option<Vec<StoredResourceObjectOperand>>,
);
impl OptionalStoredResourceObjectOperands {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredResourceObjectOperands {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<StoredResourceObjectOperand>::deserialize(d).map(|value| Self(Some(value)))
    }
}
pub(super) fn restore(
    operands: Vec<StoredResourceObjectOperand>,
) -> Result<Vec<TeletextResourceObjectOperandOverlay>, TeletextDesignDocumentError> {
    operands
        .into_iter()
        .map(|operand| {
            TeletextResourceObjectOperandOverlay::new(
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
    operands: &[TeletextResourceObjectOperandOverlay],
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
    operands: &[TeletextResourceObjectOperandOverlay],
    documents: &[TeletextProfileDocument],
) -> Result<(), TeletextDesignDocumentError> {
    for operand in operands {
        let mut definitions = documents.iter().filter_map(|document| {
            let TeletextProfileDocument::Authored(service) = document else {
                return None;
            };
            service.semantic().objects().definition(operand.identity())
        });
        let definition = definitions.next().ok_or_else(invalid)?;
        if definitions.next().is_some()
            || definition.program.commands().get(operand.command())
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

pub(super) struct BorrowedResourceObjectOperands<'a>(
    pub(super) &'a [TeletextResourceObjectOperandOverlay],
);
impl Serialize for BorrowedResourceObjectOperands<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for operand in self.0 {
            sequence.serialize_element(&StoredResourceObjectOperand {
                identity: StoredResourceObjectIdentity(operand.identity()),
                command: operand.command(),
                column: operand.column(),
                expected: StoredOperation(operand.expected()),
                replacement: StoredOperation(operand.replacement()),
            })?;
        }
        sequence.end()
    }
}

/// Restore the unchanged V14 ordering before the common attachment check.
pub(super) fn restore_overlay(
    provenance: super::TeletextSavedEditorialProvenance,
    pages: Vec<ersatzrs_domain::TeletextPhysicalBaseOverlay>,
    operands: Vec<ersatzrs_domain::TeletextLocalProgramOperandOverlay>,
    objects: Vec<ersatzrs_domain::TeletextLocalObjectOperandOverlay>,
    bitmaps: Vec<ersatzrs_domain::TeletextDrcsBitmapOverlay>,
    programs: Vec<super::local_object_glyph_programs::StoredGlyphProgram>,
    resources: Vec<StoredResourceObjectOperand>,
) -> Result<super::TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    let programs =
        super::local_object_glyph_programs::restore(programs, super::ProgramPolicy::StyleCommands)?;
    super::TeletextSavedEditorialOverlay::new_with_resource_object_operands(
        provenance,
        pages,
        operands,
        objects,
        bitmaps,
        programs,
        restore(resources)?,
    )
}
