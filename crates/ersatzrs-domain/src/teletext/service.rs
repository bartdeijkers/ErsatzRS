//! Immutable semantic service after explicitly supplied page-resource resolution.
//!
//! X27 declarations retain their required inventory, separately from the supplied
//! resolved selection. MOT/null/default derivation remains outside this boundary.
//! This checks presentation precedence, ownership and complete
//! effective-program dependencies; it is not a physical allocation or readiness
//! token. The unchanged v1 base model remains embedded rather than extended.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use super::{
    TeletextCharsetValueError, TeletextCollectedDrcsError, TeletextCollectedEnhancementReferences,
    TeletextDrcsCatalog, TeletextDrcsPageLinks, TeletextDrcsResourceError, TeletextDrcsSubtable,
    TeletextDrcsTableScope, TeletextEnhancementCollectionBudget,
    TeletextEnhancementCollectionError, TeletextEnhancementCommand,
    TeletextEnhancementObjectCatalog, TeletextEnhancementObjectDefinition,
    TeletextEnhancementProgram, TeletextEnhancementProgramError, TeletextEnhancementSource,
    TeletextLevel25PresentationDefaults, TeletextNationalOption, TeletextObjectCatalogBudget,
    TeletextObjectCatalogError, TeletextObjectIdentity, TeletextObjectScope, TeletextObjectType,
    TeletextPageAddress, TeletextPageCharsetDesignation, TeletextPageDraft, TeletextPageSetDraft,
    TeletextRawDataPageNumber, TeletextResourceKind, TeletextResourceSubpageIndex,
    TeletextRunContent, TeletextStructureError, TeletextX27Level25Links, TeletextX27LinkRole,
    TeletextX27LinkTarget,
};

#[path = "drcs_extension.rs"]
mod drcs_extension;
pub use drcs_extension::*;

#[path = "local_object_operand_service.rs"]
mod local_object_operands;

/// Explicitly resolved public/global object links, not MOT or X27 declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextResolvedObjectPageLinks {
    pub public: Option<TeletextRawDataPageNumber>,
    pub global: Option<TeletextRawDataPageNumber>,
}

/// One semantic default-object slot, retaining its independent MOT resource owner.
/// The root is not rebound to the display's explicit invocation links.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDefaultObjectSelection {
    pub target: TeletextObjectIdentity,
    pub object_type: TeletextObjectType,
}

/// Caller-supplied resolved selection for one exact page/subcode.
///
/// Slots are GPOP1, GPOP2, POP1, POP2. Their order retains Annex G.5's
/// recommended application order without claiming to parse MOT or X27 masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextResolvedPageResourceSelection {
    pub objects: TeletextResolvedObjectPageLinks,
    pub drcs: TeletextDrcsPageLinks,
    pub defaults: [Option<TeletextDefaultObjectSelection>; 4],
}

/// Page X28 presence and local X26 presence remain distinct from packet contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextResolvedPageEnhancementDraft {
    pub address: TeletextPageAddress,
    pub presentation: Option<TeletextLevel25PresentationDefaults>,
    /// Preserved authored X27/4 presence; this does not infer MOT/null precedence.
    pub compositional_links: Option<TeletextX27Level25Links>,
    /// `Some(empty)` still suppresses every supplied default-object slot.
    pub local_program: Option<TeletextEnhancementProgram>,
    pub resources: TeletextResolvedPageResourceSelection,
}

/// One present M29 declaration; absent declarations are not receiver defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMagazinePresentationDraft {
    pub magazine: u8,
    pub presentation: TeletextLevel25PresentationDefaults,
}

/// Bounded host input, separate from the eventual v2 wire and publication receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextResolvedServiceDraft {
    pub base: TeletextPageSetDraft,
    pub pages: Vec<TeletextResolvedPageEnhancementDraft>,
    pub magazines: Vec<TeletextMagazinePresentationDraft>,
    pub objects: Vec<TeletextEnhancementObjectDefinition>,
    pub drcs: Vec<TeletextDrcsSubtable>,
}

/// Trusted resource policy. Zero is meaningful; no producer can deserialize it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextServiceBudget {
    /// Base page/subpages plus unique object resources and DRCS subtables.
    pub max_records: usize,
    pub max_objects: usize,
    /// Aggregate rows, runs, navigation links, metadata records, glyphs and
    /// physical-overlay owner/row records.
    pub max_structure_items: usize,
    /// UTF-8 text bytes, one byte per six-bit mosaic cell and physical overlay
    /// cells, without truncation. Overlay storage shares the original budget.
    pub max_base_content_bytes: usize,
    /// Authored local/object commands and populated default slots, even suppressed.
    pub max_commands: usize,
    /// Aggregate expanded work across every display, including repeated calls.
    pub max_collected_commands: usize,
    pub max_collected_drcs_references: usize,
}

impl TeletextServiceBudget {
    /// Bound base input before allocating one derived enhancement record per page.
    ///
    /// This allocation-free preflight shares the service's structural/content
    /// accounting. It validates neither structure nor encoding and grants no
    /// authority: `TeletextSemanticService::new` must still check the full draft.
    /// # Errors
    /// Returns the first record, structural-item or content-byte budget failure.
    pub fn validate_base_with_page_metadata(
        self,
        base: &TeletextPageSetDraft,
    ) -> Result<(), TeletextServiceError> {
        let mut records = self.max_records;
        charge(&mut records, base.pages.len(), "record", self.max_records)?;
        let mut items = self.max_structure_items;
        for _ in 0..2 {
            charge(
                &mut items,
                base.pages.len(),
                "structure item",
                self.max_structure_items,
            )?;
        }
        let mut bytes = self.max_base_content_bytes;
        charge_base_structure_and_content(base, &mut items, &mut bytes, self)
    }
}

/// Which source provides effective page/magazine presentation data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextPresentationSource {
    Page,
    Magazine,
}

/// Borrowed presentation selection; fallback does not invent a receiver palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextEffectivePresentation<'a> {
    Extension {
        source: TeletextPresentationSource,
        defaults: &'a TeletextLevel25PresentationDefaults,
    },
    /// Annex D's ordinary Latin region-zero default, absent a country code of practice.
    Header {
        national_option: TeletextNationalOption,
    },
    /// Original associated POP flags when page/magazine presentation is absent.
    /// These effective defaults do not establish an authored X28/M29 packet.
    MotFallback {
        national_option: TeletextNationalOption,
        defaults: &'a TeletextLevel25PresentationDefaults,
    },
}

impl TeletextEffectivePresentation<'_> {
    /// Assigned primary code from the chosen extension or header national bits.
    /// # Errors
    /// Header fallback seven is reserved; explicit assigned aliases ending in
    /// seven remain valid. Language strings never choose another repertoire.
    pub fn primary_designation(
        self,
    ) -> Result<TeletextPageCharsetDesignation, TeletextCharsetValueError> {
        match self {
            Self::Extension { defaults, .. } | Self::MotFallback { defaults, .. } => {
                Ok(defaults.primary)
            }
            Self::Header { national_option } => {
                TeletextPageCharsetDesignation::new(national_option.get())
            }
        }
    }
}

/// Source of the effective program, retaining sparse default slot ordinals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeletextEffectiveProgramSource {
    Local,
    Defaults { slots: Vec<u8> },
    BaseOnly,
}

/// Located semantic failure. No programme text or partial candidate is returned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextServiceError {
    #[error("teletext DRCS bitmap changes must not be empty")]
    EmptyDrcsBitmapOverlays,
    #[error("teletext DRCS bitmap {identity:?}, slot {slot:?}: {reason}")]
    DrcsBitmapOverlay {
        identity: super::TeletextResourceIdentity,
        slot: super::TeletextDrcsGlyphSlot,
        reason: super::TeletextDrcsBitmapFailure,
    },
    #[error("teletext local operand changes must not be empty")]
    EmptyLocalProgramOperandOverlays,
    #[error("teletext local operand {address:?}, command {command}: {reason}")]
    LocalProgramOperandOverlay {
        address: TeletextPageAddress,
        command: usize,
        reason: super::TeletextLocalProgramOperandFailure,
    },
    #[error("teletext local object operand changes must not be empty")]
    EmptyLocalObjectOperandOverlays,
    #[error("teletext local object operand {identity:?}, command {command}: {reason}")]
    LocalObjectOperandOverlay {
        identity: TeletextObjectIdentity,
        command: usize,
        reason: super::TeletextLocalObjectOperandFailure,
    },
    #[error("teletext physical base overlay owner {address:?} is absent or repeated (duplicate={duplicate})")]
    PhysicalBaseOverlayOwner {
        address: TeletextPageAddress,
        duplicate: bool,
    },
    #[error("teletext service exceeds caller {dimension} budget {maximum}")]
    Budget {
        dimension: &'static str,
        maximum: usize,
    },
    #[error(transparent)]
    Base(#[from] TeletextStructureError),
    #[error("teletext service repeats enhancement metadata for {address:?}")]
    DuplicatePage { address: TeletextPageAddress },
    #[error("teletext enhancement metadata has no base owner {address:?}")]
    MissingPage { address: TeletextPageAddress },
    #[error("teletext service has invalid or repeated magazine {magazine}")]
    Magazine { magazine: u8 },
    #[error("teletext local definition {identity:?} has no base owner")]
    MissingLocalOwner { identity: TeletextObjectIdentity },
    #[error("teletext local definition {identity:?} requires an explicit local X26 declaration")]
    LocalPresence { identity: TeletextObjectIdentity },
    #[error("teletext resources collide at {page}/{subpage:?}")]
    ResourceCollision {
        page: TeletextRawDataPageNumber,
        subpage: TeletextResourceSubpageIndex,
    },
    #[error("teletext empty object companion {identity:?} must be POP or GPOP")]
    ObjectCompanionKind {
        identity: super::TeletextResourceIdentity,
    },
    #[error(transparent)]
    Objects(#[from] TeletextObjectCatalogError),
    #[error(transparent)]
    Drcs(#[from] TeletextDrcsResourceError),
    #[error("teletext display {display:?} links absent {kind:?} page {page}")]
    ObjectLink {
        display: TeletextPageAddress,
        kind: TeletextResourceKind,
        page: TeletextRawDataPageNumber,
    },
    #[error(
        "teletext display {display:?}, X27 {role:?} requires absent resource {page}/{subpage:?}"
    )]
    CompositionalResource {
        display: TeletextPageAddress,
        role: TeletextX27LinkRole,
        page: TeletextRawDataPageNumber,
        subpage: TeletextResourceSubpageIndex,
    },
    #[error(
        "teletext display {display:?}, source {source_location:?}, X27 {role:?} does not announce used resource {page}/{subpage:?}"
    )]
    CompositionalUsage {
        display: TeletextPageAddress,
        role: TeletextX27LinkRole,
        page: TeletextRawDataPageNumber,
        subpage: TeletextResourceSubpageIndex,
        source_location: TeletextEnhancementSource,
    },
    #[error(
        "teletext display {display:?}, default slot {slot}: target {target:?} has wrong scope"
    )]
    DefaultScope {
        display: TeletextPageAddress,
        slot: u8,
        target: TeletextObjectIdentity,
    },
    #[error(
        "teletext display {display:?}, source {source_location:?}: object {target:?} is outside selected resource links"
    )]
    ObjectSelection {
        display: TeletextPageAddress,
        source_location: TeletextEnhancementSource,
        target: TeletextObjectIdentity,
    },
    #[error("teletext display {display:?}: {error}")]
    Charset {
        display: TeletextPageAddress,
        error: TeletextCharsetValueError,
    },
    #[error("teletext {program_source:?} program: {error}")]
    Collection {
        program_source: TeletextEffectiveProgramSource,
        error: TeletextEnhancementCollectionError,
    },
    #[error("teletext {program_source:?} program: {error}")]
    CollectedDrcs {
        program_source: TeletextEffectiveProgramSource,
        error: TeletextCollectedDrcsError,
    },
    #[error(transparent)]
    Program(#[from] TeletextEnhancementProgramError),
}

/// Checked semantic service; no Deserialize or publication/encoder authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextSemanticService {
    base: TeletextPageSetDraft,
    pages: BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft>,
    magazines: BTreeMap<u8, TeletextLevel25PresentationDefaults>,
    mot_fallbacks: BTreeMap<TeletextPageAddress, TeletextLevel25PresentationDefaults>,
    objects: TeletextEnhancementObjectCatalog,
    object_companions: Vec<super::TeletextResourceIdentity>,
    drcs: TeletextDrcsCatalog,
    physical_base_overlays: BTreeMap<TeletextPageAddress, super::TeletextPhysicalBaseOverlay>,
    original_structure_items: usize,
    original_base_content_bytes: usize,
    collected: BTreeMap<
        TeletextPageAddress,
        (
            TeletextEffectiveProgramSource,
            TeletextCollectedEnhancementReferences,
        ),
    >,
}

impl TeletextSemanticService {
    /// Validate the complete supplied resolved selection under aggregate budgets.
    /// # Errors
    /// Rejects base/metadata/resource collisions, absent owners, invalid declared
    /// links/defaults, charset fallback and effective transitive dependencies.
    /// Byte-bounded parsing, MOT/X27 resolution, physical collisions, display
    /// semantics and encoding remain separate required steps.
    pub fn new(
        draft: TeletextResolvedServiceDraft,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        Self::new_with_object_companions(draft, Vec::new(), budget)
    }

    /// Validate explicitly empty POP/GPOP subpages without phantom definitions.
    /// Companion identities participate in the same record, structure, purpose,
    /// collision and declared-link checks as populated resources. This remains
    /// caller-resolved semantic input, not authenticated MOT or physical inventory.
    /// # Errors
    /// Rejects raw budgets first, then wrong purposes, repeated/occupied logical
    /// owners and existing semantic validation failures. No partial model escapes.
    pub fn new_with_object_companions(
        draft: TeletextResolvedServiceDraft,
        companions: Vec<super::TeletextResourceIdentity>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        Self::new_with_resolved_mot_fallbacks(draft, companions, BTreeMap::new(), budget)
    }

    // Only an already checked service can supply resolved MOT presentation here.
    // Keep it installed before collecting effective primary-repertoire references.
    fn new_with_resolved_mot_fallbacks(
        draft: TeletextResolvedServiceDraft,
        companions: Vec<super::TeletextResourceIdentity>,
        mot_fallbacks: BTreeMap<TeletextPageAddress, TeletextLevel25PresentationDefaults>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        let (original_structure_items, original_base_content_bytes) =
            check_input_budgets(&draft, &companions, budget)?;
        validate_companion_kinds(&companions)?;
        draft.base.validate_structure()?;
        let addresses: BTreeSet<_> = draft.base.pages.iter().map(|page| page.address).collect();
        let magazines = index_magazine_presentations(draft.magazines)?;
        let mut pages = BTreeMap::new();
        for page in draft.pages {
            if !addresses.contains(&page.address) {
                return Err(TeletextServiceError::MissingPage {
                    address: page.address,
                });
            }
            let address = page.address;
            if pages.insert(address, page).is_some() {
                return Err(TeletextServiceError::DuplicatePage { address });
            }
        }
        validate_local_definitions(&draft.objects, &addresses, &pages)?;
        let resource_index = check_resource_records(
            &draft.base,
            &draft.objects,
            &draft.drcs,
            &companions,
            budget.max_records,
        )?;
        let objects = TeletextEnhancementObjectCatalog::new(
            draft.objects,
            TeletextObjectCatalogBudget {
                max_objects: budget.max_objects,
                max_commands: budget.max_commands,
            },
        )?;
        let drcs = TeletextDrcsCatalog::new(draft.drcs)?;
        let mut service = Self {
            base: draft.base,
            pages,
            magazines,
            mot_fallbacks,
            objects,
            object_companions: companions,
            drcs,
            physical_base_overlays: BTreeMap::new(),
            original_structure_items,
            original_base_content_bytes,
            collected: BTreeMap::new(),
        };
        // Declared defaults are validated even when local X26 suppresses them.
        // They do not consume effective-display glyph/pair/invocation budgets.
        for page in service.pages.values() {
            validate_compositional_inventory(page, &resource_index)?;
            service.validate_links_and_defaults(page, &resource_index)?;
        }
        let mut remaining_work = budget.max_collected_commands;
        let mut remaining_references = budget.max_collected_drcs_references;
        for page in &service.base.pages {
            let primary = service
                .presentation(page)
                .primary_designation()
                .map_err(|error| TeletextServiceError::Charset {
                    display: page.address,
                    error,
                })?;
            let enhancement = service.pages.get(&page.address);
            let (program_source, program) = effective_program(enhancement)?;
            let refs = service
                .objects
                .collect_display_references(
                    page.address,
                    &program,
                    primary.pair(),
                    TeletextEnhancementCollectionBudget {
                        max_commands: remaining_work,
                        max_drcs_references: remaining_references,
                    },
                )
                .map_err(|error| TeletextServiceError::Collection {
                    program_source: program_source.clone(),
                    error,
                })?;
            if let Some(enhancement) = enhancement {
                validate_object_references(
                    page.address,
                    enhancement.resources.objects,
                    &program_source,
                    &refs,
                )?;
                refs.validate_drcs(&service.drcs, enhancement.resources.drcs)
                    .map_err(|error| TeletextServiceError::CollectedDrcs {
                        program_source: program_source.clone(),
                        error,
                    })?;
                validate_compositional_usage(enhancement, &program_source, &refs)?;
            }
            remaining_work -= refs.command_count();
            remaining_references -= refs.references().len();
            service
                .collected
                .insert(page.address, (program_source, refs));
        }
        Ok(service)
    }

    /// Apply exact-original operand changes only to existing local programs.
    ///
    /// Commands retain their ordinals, columns, families and every other operand.
    /// Resources, presentations, resolved MOT fallbacks and physical base rows are
    /// preserved. The complete service and transitive collector are revalidated
    /// under the supplied budget; no arbitrary mutable program is exposed.
    ///
    /// This transforms the semantic candidate itself, rather than storing another
    /// overlay layer. Empty input is rejected; removal requires reconstructing the
    /// original candidate. Hosts must recompile, check protected cell effects and
    /// require explicit publication. This method grants none of those permissions.
    /// # Errors
    /// Rejects raw budgets before cloning, caps indexes before insertion and rejects
    /// empty/repeated changes,
    /// absent local owners, stale/mismatched original commands and service errors.
    pub fn with_local_program_operand_overlays(
        self,
        overlays: &[super::TeletextLocalProgramOperandOverlay],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        self.check_local_operand_overlay_budgets(overlays.len(), budget)?;
        if overlays.is_empty() {
            return Err(TeletextServiceError::EmptyLocalProgramOperandOverlays);
        }
        let indexed = self.validate_local_operand_overlays(overlays)?;
        let mut pages: Vec<_> = self.pages.into_values().collect();
        for page in &mut pages {
            let mut changes = indexed
                .range((page.address, 0)..=(page.address, usize::MAX))
                .peekable();
            if changes.peek().is_none() {
                continue;
            }
            if let Some(program) = &page.local_program {
                let mut commands = program.commands().to_vec();
                for (_, overlay) in changes {
                    commands[overlay.command()] = TeletextEnhancementCommand::AtColumn {
                        column: overlay.column(),
                        operation: overlay.replacement(),
                    };
                }
                page.local_program = Some(TeletextEnhancementProgram::new(commands)?);
            }
        }
        let draft = TeletextResolvedServiceDraft {
            base: self.base,
            pages,
            magazines: self
                .magazines
                .into_iter()
                .map(
                    |(magazine, presentation)| TeletextMagazinePresentationDraft {
                        magazine,
                        presentation,
                    },
                )
                .collect(),
            objects: self.objects.definitions().to_vec(),
            drcs: self.drcs.subtables().to_vec(),
        };
        Self::new_with_resolved_mot_fallbacks(
            draft,
            self.object_companions,
            self.mot_fallbacks,
            budget,
        )?
        .with_physical_base_overlays(self.physical_base_overlays.into_values().collect(), budget)
    }

    /// Replace existing DRCS glyph bitmaps under exact original expectations.
    ///
    /// Resource identities, glyph order, local programs, selections and physical
    /// overlays remain unchanged. The complete semantic service is revalidated.
    /// Shared glyph effects, editor permissions and publication are not authorized
    /// here; hosts must check those after compiling the complete candidate.
    /// # Errors
    /// Rejects empty/repeated changes, missing resources or glyphs, stale bitmaps
    /// and service budgets before cloning resource contents or allocating indexes.
    pub fn with_drcs_bitmap_overlays(
        self,
        overlays: &[super::TeletextDrcsBitmapOverlay],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        let mut changes = budget.max_structure_items;
        charge(
            &mut changes,
            overlays.len(),
            "structure item",
            budget.max_structure_items,
        )?;
        self.check_existing_service_budgets(budget)?;
        if overlays.is_empty() {
            return Err(TeletextServiceError::EmptyDrcsBitmapOverlays);
        }
        let indexed = self.validate_drcs_bitmap_overlays(overlays)?;
        let mut subtables = Vec::with_capacity(self.drcs.subtables().len());
        for table in self.drcs.subtables() {
            let mut glyphs = table.glyphs().to_vec();
            for glyph in &mut glyphs {
                if let Some(overlay) = indexed.get(&(table.identity(), glyph.slot)) {
                    glyph.bitmap = overlay.replacement();
                }
            }
            subtables.push(TeletextDrcsSubtable::new(table.identity(), glyphs)?);
        }
        let draft = TeletextResolvedServiceDraft {
            base: self.base,
            pages: self.pages.into_values().collect(),
            magazines: self
                .magazines
                .into_iter()
                .map(
                    |(magazine, presentation)| TeletextMagazinePresentationDraft {
                        magazine,
                        presentation,
                    },
                )
                .collect(),
            objects: self.objects.definitions().to_vec(),
            drcs: subtables,
        };
        Self::new_with_resolved_mot_fallbacks(
            draft,
            self.object_companions,
            self.mot_fallbacks,
            budget,
        )?
        .with_physical_base_overlays(self.physical_base_overlays.into_values().collect(), budget)
    }

    fn validate_drcs_bitmap_overlays<'a>(
        &self,
        overlays: &'a [super::TeletextDrcsBitmapOverlay],
    ) -> Result<
        BTreeMap<
            (
                super::TeletextResourceIdentity,
                super::TeletextDrcsGlyphSlot,
            ),
            &'a super::TeletextDrcsBitmapOverlay,
        >,
        TeletextServiceError,
    > {
        let mut indexed = BTreeMap::new();
        for overlay in overlays {
            let failure = |reason| TeletextServiceError::DrcsBitmapOverlay {
                identity: overlay.identity(),
                slot: overlay.slot(),
                reason,
            };
            if indexed
                .insert((overlay.identity(), overlay.slot()), overlay)
                .is_some()
            {
                return Err(failure(super::TeletextDrcsBitmapFailure::Duplicate));
            }
            let table = self
                .drcs
                .subtables()
                .iter()
                .find(|table| table.identity() == overlay.identity())
                .ok_or_else(|| failure(super::TeletextDrcsBitmapFailure::MissingResource))?;
            let glyph = table
                .glyphs()
                .iter()
                .find(|glyph| glyph.slot == overlay.slot())
                .ok_or_else(|| failure(super::TeletextDrcsBitmapFailure::MissingGlyph))?;
            if glyph.bitmap != overlay.expected() {
                return Err(failure(super::TeletextDrcsBitmapFailure::BitmapMismatch));
            }
        }
        Ok(indexed)
    }

    fn validate_local_operand_overlays<'a>(
        &self,
        overlays: &'a [super::TeletextLocalProgramOperandOverlay],
    ) -> Result<
        BTreeMap<(TeletextPageAddress, usize), &'a super::TeletextLocalProgramOperandOverlay>,
        TeletextServiceError,
    > {
        let mut indexed = BTreeMap::new();
        for overlay in overlays {
            let failure = |reason| TeletextServiceError::LocalProgramOperandOverlay {
                address: overlay.owner(),
                command: overlay.command(),
                reason,
            };
            if indexed
                .insert((overlay.owner(), overlay.command()), overlay)
                .is_some()
            {
                return Err(failure(
                    super::TeletextLocalProgramOperandFailure::Duplicate,
                ));
            }
            let program = self
                .pages
                .get(&overlay.owner())
                .and_then(|page| page.local_program.as_ref())
                .ok_or_else(|| {
                    failure(super::TeletextLocalProgramOperandFailure::MissingLocalProgram)
                })?;
            if program.commands().get(overlay.command())
                != Some(&TeletextEnhancementCommand::AtColumn {
                    column: overlay.column(),
                    operation: overlay.expected(),
                })
            {
                return Err(failure(
                    super::TeletextLocalProgramOperandFailure::CommandMismatch,
                ));
            }
        }
        Ok(indexed)
    }

    fn check_local_operand_overlay_budgets(
        &self,
        overlay_count: usize,
        budget: TeletextServiceBudget,
    ) -> Result<(), TeletextServiceError> {
        // Raw replacement input cannot exceed the same authored-command budget.
        let mut changes = budget.max_commands;
        charge(
            &mut changes,
            overlay_count,
            "authored command",
            budget.max_commands,
        )?;
        self.check_existing_service_budgets(budget)
    }

    fn check_existing_service_budgets(
        &self,
        budget: TeletextServiceBudget,
    ) -> Result<(), TeletextServiceError> {
        check_borrowed_input_budgets(
            &self.base,
            self.pages.values(),
            self.magazines.len(),
            self.objects.definitions(),
            self.drcs.subtables(),
            &self.object_companions,
            budget,
        )?;
        check_physical_overlay_budgets(
            self.original_structure_items,
            self.original_base_content_bytes,
            self.physical_base_overlays.values(),
            budget,
        )?;
        // Reuse the ordinary collision/count checker: it charges each unique
        // resource before allocating an entry, even under a reduced record cap.
        check_resource_records(
            &self.base,
            self.objects.definitions(),
            self.drcs.subtables(),
            &self.object_companions,
            budget.max_records,
        )?;
        let mut commands = budget.max_collected_commands;
        let mut references = budget.max_collected_drcs_references;
        for (_, collected) in self.collected.values() {
            charge(
                &mut commands,
                collected.command_count(),
                "collected command",
                budget.max_collected_commands,
            )?;
            charge(
                &mut references,
                collected.references().len(),
                "collected DRCS reference",
                budget.max_collected_drcs_references,
            )?;
        }
        Ok(())
    }

    #[must_use]
    pub const fn base(&self) -> &TeletextPageSetDraft {
        &self.base
    }
    /// Replace the complete physical-row overlay set without modifying semantic
    /// metadata, logical resource owners or previously resolved dependencies.
    /// Raw counts and exact owners are checked before allocating the new index.
    /// # Errors
    /// Rejects absent/repeated owners and the combined original-plus-overlay
    /// structural/content footprint. Empty input removes every overlay.
    pub fn with_physical_base_overlays(
        mut self,
        overlays: Vec<super::TeletextPhysicalBaseOverlay>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        check_physical_overlay_budgets(
            self.original_structure_items,
            self.original_base_content_bytes,
            overlays.iter(),
            budget,
        )?;
        for (ordinal, overlay) in overlays.iter().enumerate() {
            if !self
                .base
                .pages
                .iter()
                .any(|page| page.address == overlay.owner())
            {
                return Err(TeletextServiceError::PhysicalBaseOverlayOwner {
                    address: overlay.owner(),
                    duplicate: false,
                });
            }
            if overlays[..ordinal]
                .iter()
                .any(|previous| previous.owner() == overlay.owner())
            {
                return Err(TeletextServiceError::PhysicalBaseOverlayOwner {
                    address: overlay.owner(),
                    duplicate: true,
                });
            }
        }
        self.physical_base_overlays = overlays
            .into_iter()
            .map(|overlay| (overlay.owner(), overlay))
            .collect();
        Ok(self)
    }
    #[must_use]
    pub fn physical_base_overlay(
        &self,
        owner: TeletextPageAddress,
    ) -> Option<&super::TeletextPhysicalBaseOverlay> {
        self.physical_base_overlays.get(&owner)
    }
    #[must_use]
    pub const fn physical_base_overlays(
        &self,
    ) -> &BTreeMap<TeletextPageAddress, super::TeletextPhysicalBaseOverlay> {
        &self.physical_base_overlays
    }
    #[must_use]
    pub fn page_enhancements(
        &self,
    ) -> &BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft> {
        &self.pages
    }
    #[must_use]
    pub fn magazine_presentations(&self) -> &BTreeMap<u8, TeletextLevel25PresentationDefaults> {
        &self.magazines
    }
    #[must_use]
    pub const fn objects(&self) -> &TeletextEnhancementObjectCatalog {
        &self.objects
    }
    /// Explicit empty resource owners, retaining input order and purpose.
    #[must_use]
    pub fn object_companions(&self) -> &[super::TeletextResourceIdentity] {
        &self.object_companions
    }
    #[must_use]
    pub const fn drcs(&self) -> &TeletextDrcsCatalog {
        &self.drcs
    }
    #[must_use]
    pub fn collected(
        &self,
        address: TeletextPageAddress,
    ) -> Option<&(
        TeletextEffectiveProgramSource,
        TeletextCollectedEnhancementReferences,
    )> {
        self.collected.get(&address)
    }

    fn presentation(&self, page: &TeletextPageDraft) -> TeletextEffectivePresentation<'_> {
        if let Some(defaults) = self
            .pages
            .get(&page.address)
            .and_then(|page| page.presentation.as_ref())
        {
            return TeletextEffectivePresentation::Extension {
                source: TeletextPresentationSource::Page,
                defaults,
            };
        }
        if let Some(defaults) = self.magazines.get(&page.address.page.magazine()) {
            return TeletextEffectivePresentation::Extension {
                source: TeletextPresentationSource::Magazine,
                defaults,
            };
        }
        if let Some(defaults) = self.mot_fallbacks.get(&page.address) {
            return TeletextEffectivePresentation::MotFallback {
                national_option: page.national_option,
                defaults,
            };
        }
        TeletextEffectivePresentation::Header {
            national_option: page.national_option,
        }
    }

    /// Rebinding actual declarations must not inherit an earlier MOT's defaults.
    /// The owning declaration factory calls this after all correspondence checks.
    pub(super) fn clear_mot_fallbacks(&mut self) {
        self.mot_fallbacks.clear();
    }

    /// Only the checked declaration factory can attach original POP fallback.
    /// Actual page and magazine declarations remain untouched and take precedence.
    pub(super) fn set_mot_fallback(
        &mut self,
        address: TeletextPageAddress,
        defaults: TeletextLevel25PresentationDefaults,
    ) {
        self.mot_fallbacks.insert(address, defaults);
    }

    /// Effective metadata for an actual base owner; unknown addresses return None.
    #[must_use]
    pub fn effective_presentation(
        &self,
        address: TeletextPageAddress,
    ) -> Option<TeletextEffectivePresentation<'_>> {
        self.base
            .pages
            .iter()
            .find(|page| page.address == address)
            .map(|page| self.presentation(page))
    }

    /// Effective ordered program for one exact existing base page/subcode.
    ///
    /// Present local programs, including an empty override, remain borrowed and
    /// suppress all declared defaults. Without local X26, the existing selection
    /// policy synthesizes owned default invocations in canonical sparse slot
    /// order, or an owned empty `BaseOnly` program. This exposes already validated
    /// semantic selection; it grants no physical encoding or publication authority.
    /// # Errors
    /// Returns `MissingPage` for an unknown exact owner, including another subcode
    /// of an existing page. It does not fabricate an empty program for that owner.
    pub fn effective_program(
        &self,
        address: TeletextPageAddress,
    ) -> Result<
        (
            TeletextEffectiveProgramSource,
            Cow<'_, TeletextEnhancementProgram>,
        ),
        TeletextServiceError,
    > {
        if !self.base.pages.iter().any(|page| page.address == address) {
            return Err(TeletextServiceError::MissingPage { address });
        }
        effective_program(self.pages.get(&address))
    }

    fn validate_links_and_defaults(
        &self,
        page: &TeletextResolvedPageEnhancementDraft,
        resources: &BTreeMap<
            (TeletextRawDataPageNumber, TeletextResourceSubpageIndex),
            TeletextResourceKind,
        >,
    ) -> Result<(), TeletextServiceError> {
        for (kind, link) in [
            (TeletextResourceKind::Pop, page.resources.objects.public),
            (TeletextResourceKind::Gpop, page.resources.objects.global),
        ] {
            if let Some(link) = link {
                if !resources
                    .iter()
                    .any(|((number, _), purpose)| *number == link && *purpose == kind)
                {
                    return Err(TeletextServiceError::ObjectLink {
                        display: page.address,
                        kind,
                        page: link,
                    });
                }
            }
        }
        for (slot, selection) in (0_u8..4).zip(page.resources.defaults) {
            if let Some(selection) = selection {
                let scope = if slot < 2 {
                    TeletextObjectScope::Global
                } else {
                    TeletextObjectScope::Public
                };
                if selection.target.scope() != scope {
                    return Err(TeletextServiceError::DefaultScope {
                        display: page.address,
                        slot,
                        target: selection.target,
                    });
                }
                // Only the declaration's target/type is checked here. Its graph
                // closure is already checked; traversal would charge suppressed
                // glyphs/charsets against an unrelated effective page.
                let definition = self.objects.definition(selection.target);
                let error = match definition {
                    None => Some(super::TeletextObjectInvocationError::Missing {
                        target: selection.target,
                    }),
                    Some(definition) if definition.object_type != selection.object_type => {
                        Some(super::TeletextObjectInvocationError::Type {
                            target: selection.target,
                            expected: selection.object_type,
                            actual: definition.object_type,
                        })
                    }
                    _ => None,
                };
                if let Some(reason) = error {
                    return Err(TeletextServiceError::Collection {
                        program_source: TeletextEffectiveProgramSource::Defaults {
                            slots: vec![slot],
                        },
                        error: TeletextEnhancementCollectionError {
                            display: page.address,
                            source_location: TeletextEnhancementSource {
                                object: None,
                                command: 0,
                            },
                            reason: super::TeletextEnhancementCollectionFailure::Invocation(reason),
                        },
                    });
                }
                // EN 300 706 13.8 ties a default root to its original MOT
                // resource, independently of X27's explicit invocation links.
                // Its exact definition/type and scope were checked above.
            }
        }
        // Even unused declared DRCS links must name existing resource pages.
        self.drcs
            .validate_display_usage(page.address, page.resources.drcs, &[])?;
        Ok(())
    }
}

fn validate_compositional_inventory(
    page: &TeletextResolvedPageEnhancementDraft,
    resources: &BTreeMap<
        (TeletextRawDataPageNumber, TeletextResourceSubpageIndex),
        TeletextResourceKind,
    >,
) -> Result<(), TeletextServiceError> {
    let Some(links) = page.compositional_links else {
        return Ok(());
    };
    for role in TeletextX27LinkRole::ALL {
        let link = links.link(role);
        if !link.validity().applies_at_level25() {
            continue;
        }
        let TeletextX27LinkTarget::Page { page: target } = link.target() else {
            // Explicit null is retained; it neither demands sixteen records
            // nor settles the separate MOT/null fallback policy.
            continue;
        };
        for subpage in TeletextResourceSubpageIndex::ALL {
            if link.required_subpages().requires(subpage)
                && resources.get(&(target, subpage)) != Some(&role.kind())
            {
                return Err(TeletextServiceError::CompositionalResource {
                    display: page.address,
                    role,
                    page: target,
                    subpage,
                });
            }
        }
    }
    Ok(())
}

/// Check only an applicable declaration for the same supplied selected owner.
/// A different owner or null still requires the separate MOT/X27 resolver.
fn validate_compositional_usage(
    page: &TeletextResolvedPageEnhancementDraft,
    program_source: &TeletextEffectiveProgramSource,
    refs: &TeletextCollectedEnhancementReferences,
) -> Result<(), TeletextServiceError> {
    let Some(links) = page.compositional_links else {
        return Ok(());
    };
    let check = |role: TeletextX27LinkRole,
                 target: TeletextRawDataPageNumber,
                 subpage: TeletextResourceSubpageIndex,
                 source_location: TeletextEnhancementSource| {
        let link = links.link(role);
        if link.validity().applies_at_level25()
            && link.target() == (TeletextX27LinkTarget::Page { page: target })
            && !link.required_subpages().requires(subpage)
        {
            return Err(TeletextServiceError::CompositionalUsage {
                display: page.address,
                role,
                page: target,
                subpage,
                source_location,
            });
        }
        Ok(())
    };
    for invocation in refs.object_references() {
        if matches!(
            program_source,
            TeletextEffectiveProgramSource::Defaults { .. }
        ) && invocation.source_location.object.is_none()
        {
            // Original MOT defaults are not explicit X27-selected root calls.
            continue;
        }
        if let Some(resource) = invocation.target.resource() {
            let role = match invocation.target.scope() {
                TeletextObjectScope::Public => TeletextX27LinkRole::Pop,
                TeletextObjectScope::Global => TeletextX27LinkRole::Gpop,
                TeletextObjectScope::Local => continue,
            };
            check(
                role,
                resource.page(),
                resource.logical_subpage(),
                invocation.source_location,
            )?;
        }
    }
    for (reference, source_location) in refs.references().iter().zip(refs.sources()) {
        if let Some(target) = page.resources.drcs.page(reference.table) {
            let role = match reference.table {
                TeletextDrcsTableScope::Normal => TeletextX27LinkRole::Drcs,
                TeletextDrcsTableScope::Global => TeletextX27LinkRole::Gdrcs,
            };
            check(role, target, reference.subtable.into(), *source_location)?;
        }
    }
    Ok(())
}

fn validate_local_definitions(
    objects: &[TeletextEnhancementObjectDefinition],
    addresses: &BTreeSet<TeletextPageAddress>,
    pages: &BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft>,
) -> Result<(), TeletextServiceError> {
    for definition in objects {
        if let Some(owner) = definition.identity.owner() {
            if !addresses.contains(&owner) {
                return Err(TeletextServiceError::MissingLocalOwner {
                    identity: definition.identity,
                });
            }
            if pages
                .get(&owner)
                .is_none_or(|page| page.local_program.is_none())
            {
                return Err(TeletextServiceError::LocalPresence {
                    identity: definition.identity,
                });
            }
        }
    }
    Ok(())
}

fn invoke(selection: TeletextDefaultObjectSelection) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: selection.target,
        object_type: selection.object_type,
        offset: None,
    }
}

fn effective_program(
    page: Option<&TeletextResolvedPageEnhancementDraft>,
) -> Result<
    (
        TeletextEffectiveProgramSource,
        Cow<'_, TeletextEnhancementProgram>,
    ),
    TeletextServiceError,
> {
    if let Some(program) = page.and_then(|page| page.local_program.as_ref()) {
        return Ok((
            TeletextEffectiveProgramSource::Local,
            Cow::Borrowed(program),
        ));
    }
    let mut commands = Vec::new();
    let mut slots = Vec::new();
    if let Some(page) = page {
        for (slot, selection) in (0_u8..4).zip(page.resources.defaults) {
            if let Some(selection) = selection {
                slots.push(slot);
                commands.push(invoke(selection));
            }
        }
    }
    let source = if slots.is_empty() {
        TeletextEffectiveProgramSource::BaseOnly
    } else {
        TeletextEffectiveProgramSource::Defaults { slots }
    };
    Ok((
        source,
        Cow::Owned(TeletextEnhancementProgram::new(commands)?),
    ))
}

fn validate_object_references(
    display: TeletextPageAddress,
    links: TeletextResolvedObjectPageLinks,
    program_source: &TeletextEffectiveProgramSource,
    refs: &TeletextCollectedEnhancementReferences,
) -> Result<(), TeletextServiceError> {
    for invocation in refs.object_references() {
        if matches!(
            program_source,
            TeletextEffectiveProgramSource::Defaults { .. }
        ) && invocation.source_location.object.is_none()
        {
            // Effective defaults synthesize only root invocations. Nested
            // commands retain object attribution and the supplied link policy.
            continue;
        }
        validate_object_target(
            display,
            links,
            invocation.target,
            invocation.source_location,
        )?;
    }
    Ok(())
}

fn validate_object_target(
    display: TeletextPageAddress,
    links: TeletextResolvedObjectPageLinks,
    target: TeletextObjectIdentity,
    source_location: TeletextEnhancementSource,
) -> Result<(), TeletextServiceError> {
    if let Some(resource) = target.resource() {
        let link = match target.scope() {
            TeletextObjectScope::Public => links.public,
            TeletextObjectScope::Global => links.global,
            TeletextObjectScope::Local => None,
        };
        if link != Some(resource.page()) {
            return Err(TeletextServiceError::ObjectSelection {
                display,
                source_location,
                target,
            });
        }
    }
    Ok(())
}

fn charge(
    remaining: &mut usize,
    amount: usize,
    dimension: &'static str,
    maximum: usize,
) -> Result<(), TeletextServiceError> {
    *remaining = remaining
        .checked_sub(amount)
        .ok_or(TeletextServiceError::Budget { dimension, maximum })?;
    Ok(())
}

fn validate_companion_kinds(
    companions: &[super::TeletextResourceIdentity],
) -> Result<(), TeletextServiceError> {
    for &identity in companions {
        if !matches!(
            identity.kind(),
            TeletextResourceKind::Pop | TeletextResourceKind::Gpop
        ) {
            return Err(TeletextServiceError::ObjectCompanionKind { identity });
        }
    }
    Ok(())
}

fn index_magazine_presentations(
    declarations: Vec<TeletextMagazinePresentationDraft>,
) -> Result<BTreeMap<u8, TeletextLevel25PresentationDefaults>, TeletextServiceError> {
    let mut magazines = BTreeMap::new();
    for declaration in declarations {
        if !(1..=8).contains(&declaration.magazine)
            || magazines
                .insert(declaration.magazine, declaration.presentation)
                .is_some()
        {
            return Err(TeletextServiceError::Magazine {
                magazine: declaration.magazine,
            });
        }
    }
    Ok(magazines)
}

fn check_input_budgets(
    draft: &TeletextResolvedServiceDraft,
    companions: &[super::TeletextResourceIdentity],
    budget: TeletextServiceBudget,
) -> Result<(usize, usize), TeletextServiceError> {
    check_borrowed_input_budgets(
        &draft.base,
        draft.pages.iter(),
        draft.magazines.len(),
        &draft.objects,
        &draft.drcs,
        companions,
        budget,
    )
}

fn check_borrowed_input_budgets<'a>(
    base: &TeletextPageSetDraft,
    pages: impl ExactSizeIterator<Item = &'a TeletextResolvedPageEnhancementDraft>,
    magazine_count: usize,
    objects: &[TeletextEnhancementObjectDefinition],
    drcs: &[TeletextDrcsSubtable],
    companions: &[super::TeletextResourceIdentity],
    budget: TeletextServiceBudget,
) -> Result<(usize, usize), TeletextServiceError> {
    let mut record_count = budget.max_records;
    charge(
        &mut record_count,
        base.pages.len(),
        "record",
        budget.max_records,
    )?;
    charge(&mut record_count, drcs.len(), "record", budget.max_records)?;
    charge(
        &mut record_count,
        companions.len(),
        "record",
        budget.max_records,
    )?;
    let mut object_count = budget.max_objects;
    charge(
        &mut object_count,
        objects.len(),
        "object",
        budget.max_objects,
    )?;
    let mut items = budget.max_structure_items;
    let mut bytes = budget.max_base_content_bytes;
    let mut commands = budget.max_commands;
    charge(
        &mut items,
        base.pages.len(),
        "structure item",
        budget.max_structure_items,
    )?;
    charge(
        &mut items,
        pages.len(),
        "structure item",
        budget.max_structure_items,
    )?;
    charge(
        &mut items,
        magazine_count,
        "structure item",
        budget.max_structure_items,
    )?;
    charge(
        &mut items,
        drcs.len(),
        "structure item",
        budget.max_structure_items,
    )?;
    charge(
        &mut items,
        companions.len(),
        "structure item",
        budget.max_structure_items,
    )?;
    charge_base_structure_and_content(base, &mut items, &mut bytes, budget)?;
    for page in pages {
        if let Some(program) = &page.local_program {
            charge(
                &mut commands,
                program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        charge(
            &mut commands,
            page.resources.defaults.iter().flatten().count(),
            "authored command",
            budget.max_commands,
        )?;
    }
    for object in objects {
        charge(
            &mut commands,
            object.program.commands().len(),
            "authored command",
            budget.max_commands,
        )?;
    }
    for subtable in drcs {
        charge(
            &mut items,
            subtable.glyphs().len(),
            "structure item",
            budget.max_structure_items,
        )?;
    }
    Ok((
        budget.max_structure_items - items,
        budget.max_base_content_bytes - bytes,
    ))
}

fn charge_base_structure_and_content(
    base: &TeletextPageSetDraft,
    items: &mut usize,
    bytes: &mut usize,
    budget: TeletextServiceBudget,
) -> Result<(), TeletextServiceError> {
    for page in &base.pages {
        charge(
            items,
            page.rows.len(),
            "structure item",
            budget.max_structure_items,
        )?;
        charge(
            items,
            page.navigation.len(),
            "structure item",
            budget.max_structure_items,
        )?;
        for row in &page.rows {
            charge(
                items,
                row.runs.len(),
                "structure item",
                budget.max_structure_items,
            )?;
            for run in &row.runs {
                let amount = match &run.content {
                    TeletextRunContent::Text { text } => text.len(),
                    TeletextRunContent::Mosaics { cells, .. } => cells.len(),
                };
                charge(
                    bytes,
                    amount,
                    "base content byte",
                    budget.max_base_content_bytes,
                )?;
            }
        }
    }
    Ok(())
}

fn check_resource_records(
    base: &TeletextPageSetDraft,
    objects: &[TeletextEnhancementObjectDefinition],
    drcs: &[TeletextDrcsSubtable],
    companions: &[super::TeletextResourceIdentity],
    maximum: usize,
) -> Result<
    BTreeMap<(TeletextRawDataPageNumber, TeletextResourceSubpageIndex), TeletextResourceKind>,
    TeletextServiceError,
> {
    let mut remaining = maximum;
    charge(&mut remaining, base.pages.len(), "record", maximum)?;
    let mut index = BTreeMap::new();
    for (identity, repeatable) in drcs
        .iter()
        .map(|table| (table.identity(), false))
        .chain(
            objects
                .iter()
                .filter_map(|object| object.identity.resource().map(|identity| (identity, true))),
        )
        .chain(companions.iter().copied().map(|identity| (identity, false)))
    {
        let key = (identity.page(), identity.logical_subpage());
        if let Some(kind) = index.get(&key) {
            if repeatable && *kind == identity.kind() {
                continue;
            }
            return Err(TeletextServiceError::ResourceCollision {
                page: key.0,
                subpage: key.1,
            });
        }
        charge(&mut remaining, 1, "record", maximum)?;
        index.insert(key, identity.kind());
    }
    Ok(index)
}

fn check_physical_overlay_budgets<'a>(
    original_structure_items: usize,
    original_base_content_bytes: usize,
    overlays: impl ExactSizeIterator<Item = &'a super::TeletextPhysicalBaseOverlay>,
    budget: TeletextServiceBudget,
) -> Result<(), TeletextServiceError> {
    let mut items = budget.max_structure_items;
    let mut bytes = budget.max_base_content_bytes;
    charge(
        &mut items,
        original_structure_items,
        "structure item",
        budget.max_structure_items,
    )?;
    charge(
        &mut bytes,
        original_base_content_bytes,
        "base content byte",
        budget.max_base_content_bytes,
    )?;
    charge(
        &mut items,
        overlays.len(),
        "structure item",
        budget.max_structure_items,
    )?;
    for overlay in overlays {
        charge(
            &mut items,
            overlay.rows().len(),
            "structure item",
            budget.max_structure_items,
        )?;
        for _ in overlay.rows() {
            charge(
                &mut bytes,
                40,
                "base content byte",
                budget.max_base_content_bytes,
            )?;
        }
    }
    Ok(())
}
