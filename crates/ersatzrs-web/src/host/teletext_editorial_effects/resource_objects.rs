//! Resource packet changes must be the complete original-bound native encoding.
use super::{check_deadline, error, Result, TeletextEditorialEffectsFailure};
use ersatzrs_domain::{TeletextEnhancementObjectCatalog, TeletextResourceObjectDefinitionAddition};
use ersatzrs_ffmpeg::teletext::{
    allocate_teletext_resource_objects, encode_teletext_object_page, EncodedTeletextAuthoredPage,
    EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingBudget,
    TeletextObjectPageTransmission, TeletextResourceFullUpdate, TeletextResourceObjectAllocations,
};
use std::time::Instant;

fn structure() -> super::TeletextEditorialEffectsError {
    error(
        None,
        None,
        TeletextEditorialEffectsFailure::ProtectedStructure,
    )
}

pub(super) fn checked_context<'a>(
    original: &'a EncodedTeletextAuthoredService,
    edited: &'a EncodedTeletextAuthoredService,
    incoming: super::TeletextEditorialEdits<'_>,
    cumulative: super::TeletextEditorialEdits<'_>,
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<Option<Context<'a>>> {
    let resource_context = if incoming.resource_operands.is_empty()
        && incoming.resource_programs.is_empty()
        && incoming.resource_additions.is_empty()
        && incoming.object_resource_additions.is_empty()
        && cumulative.object_resource_additions.is_empty()
    {
        None
    } else {
        Some(Context::new(
            original,
            edited,
            incoming.resource_additions,
            incoming.object_resource_additions,
            budget,
            deadline,
        )?)
    };
    if let Some(context) = resource_context.as_ref() {
        for addition in cumulative.object_resource_additions {
            context.check_added(edited, addition, deadline)?;
        }
    }
    Ok(resource_context)
}

/// One bounded allocation pair serves every changed resource in this comparison.
/// Complete semantic correspondence is checked before constructing this context.
pub(super) struct Context<'a> {
    before: &'a TeletextEnhancementObjectCatalog,
    after: &'a TeletextEnhancementObjectCatalog,
    before_allocations: TeletextResourceObjectAllocations,
    after_allocations: TeletextResourceObjectAllocations,
    max_commands: usize,
}
impl<'a> Context<'a> {
    pub(super) fn new(
        original: &'a EncodedTeletextAuthoredService,
        edited: &'a EncodedTeletextAuthoredService,
        additions: &[TeletextResourceObjectDefinitionAddition],
        banks: &[ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition],
        budget: &TeletextAuthoredServiceEncodingBudget,
        deadline: Instant,
    ) -> Result<Self> {
        check_deadline(deadline)?;
        let declared = original.declared();
        let before = declared.semantic().objects();
        let after = edited.declared().semantic().objects();
        let maximum = budget.normalization.max_resource_objects;
        if before.definitions().len() > maximum
            || after.definitions().len() > maximum
            || declared.defaults().len() > maximum
            || additions.len() > maximum
        {
            return Err(error(None, None, TeletextEditorialEffectsFailure::Budget));
        }
        if banks.is_empty() {
            super::definition_additions::check_incoming(before, after, additions, deadline)?;
            super::definition_additions::check_declarations(
                declared,
                edited.declared(),
                additions,
                deadline,
            )?;
        }
        if declared.defaults() != edited.declared().defaults() {
            return Err(structure());
        }
        for catalog in [before, after] {
            let mut remaining = budget.max_commands;
            for definition in catalog.definitions() {
                check_deadline(deadline)?;
                remaining = remaining
                    .checked_sub(definition.program.commands().len())
                    .ok_or_else(|| error(None, None, TeletextEditorialEffectsFailure::Budget))?;
            }
        }
        let defaults = declared
            .defaults()
            .iter()
            .map(ersatzrs_domain::TeletextDeclaredDefaultReservation::selection)
            .collect::<Vec<_>>();
        let before_allocations = allocate_teletext_resource_objects(before, &defaults, maximum)
            .map_err(|_| structure())?;
        check_deadline(deadline)?;
        let after_allocations = allocate_teletext_resource_objects(after, &defaults, maximum)
            .map_err(|_| structure())?;
        check_deadline(deadline)?;
        if additions.is_empty() && banks.is_empty() && before_allocations != after_allocations {
            return Err(structure());
        }
        Ok(Self {
            before,
            after,
            before_allocations,
            after_allocations,
            max_commands: budget.max_commands,
        })
    }

    /// New resources use the explicit intent's entire header and canonical body.
    pub(super) fn check_added(
        &self,
        edited: &EncodedTeletextAuthoredService,
        addition: &ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition,
        deadline: Instant,
    ) -> Result<()> {
        check_deadline(deadline)?;
        let lifecycle = addition.transmission();
        let transmission = TeletextObjectPageTransmission {
            counter: lifecycle.counter,
            update: match lifecycle.update {
                ersatzrs_app::teletext_design_documents::TeletextSavedResourceUpdate::Changed => {
                    TeletextResourceFullUpdate::Changed
                }
                ersatzrs_app::teletext_design_documents::TeletextSavedResourceUpdate::Unchanged => {
                    TeletextResourceFullUpdate::Unchanged
                }
            },
            mode: ersatzrs_ffmpeg::teletext::TeletextResourceTransmissionMode::Serial,
            last_subpage: lifecycle.last_subpage,
        };
        let expected = encode_teletext_object_page(
            addition.addition().resource(),
            self.after,
            &self.after_allocations,
            transmission,
            self.max_commands,
        )
        .map_err(|_| structure())?;
        let mut pages = edited.pages().iter().filter(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == addition.addition().resource())
        });
        if !matches!(pages.next(), Some(EncodedTeletextAuthoredPage::Object(actual)) if actual == &expected)
            || pages.next().is_some()
        {
            return Err(structure());
        }
        check_deadline(deadline)
    }

    /// Authenticate the immutable original against complete canonical output for
    /// its existing last-subpage value, then encode only the exact edited catalogue
    /// with independently authenticated allocations, full header and transmission
    /// parameters. Only explicitly added definitions may change allocation.
    pub(super) fn check(
        &self,
        before: &EncodedTeletextAuthoredPage,
        after: &EncodedTeletextAuthoredPage,
        deadline: Instant,
    ) -> Result<()> {
        check_deadline(deadline)?;
        let (
            EncodedTeletextAuthoredPage::Object(before),
            EncodedTeletextAuthoredPage::Object(after),
        ) = (before, after)
        else {
            return Err(structure());
        };
        // Canonical old/new encodings below authenticate derived last-packet
        // extents and pointer relocation; identity/lifecycle are not upload authority.
        if before.header().identity() != after.header().identity()
            || before.transmission_mode() != after.transmission_mode()
        {
            return Err(structure());
        }
        let header = before.header();
        let update = if header.transmission().update_indicator() {
            TeletextResourceFullUpdate::Changed
        } else {
            TeletextResourceFullUpdate::Unchanged
        };
        for last_subpage in [false, true] {
            check_deadline(deadline)?;
            let transmission = TeletextObjectPageTransmission {
                counter: header.update_counter(),
                update,
                mode: before.transmission_mode(),
                last_subpage,
            };
            let expected_original = encode_teletext_object_page(
                header.identity(),
                self.before,
                &self.before_allocations,
                transmission,
                self.max_commands,
            )
            .map_err(|_| structure())?;
            check_deadline(deadline)?;
            if before != &expected_original {
                continue;
            }
            let expected = encode_teletext_object_page(
                header.identity(),
                self.after,
                &self.after_allocations,
                transmission,
                self.max_commands,
            )
            .map_err(|_| structure())?;
            check_deadline(deadline)?;
            return if after == &expected {
                Ok(())
            } else {
                Err(structure())
            };
        }
        Err(structure())
    }
}
