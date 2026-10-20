//! Authentication of actual MOT/X27 declarations against a resolved service.
//!
//! This owns one immutable input generation. It proves declaration correspondence
//! and logical resource completeness, not physical packing, lifecycle, transport
//! or receiver acceptance. Original conditional POP presentation is applied only
//! without actual page/magazine declarations; an unproven unused-link effect is
//! reported explicitly rather than guessed.

use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};

use super::{
    TeletextDefaultObjectSelection, TeletextMotConditionalPopFallback, TeletextMotDefaultObject,
    TeletextMotLevel25Declaration, TeletextMotLevel25Resolution, TeletextMotLinkProvenance,
    TeletextMotLinkRole, TeletextMotPublicLinkIndex, TeletextMotResolutionError,
    TeletextMotRoleResolution, TeletextMotUnresolvedReason, TeletextPageAddress,
    TeletextResolvedPageResourceSelection, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex, TeletextSemanticService, TeletextSubpageRequirementMask,
    TeletextX27Level25Links, TeletextX27LinkRole, TeletextX27LinkTarget,
};

/// Exact display subcode and explicitly selected MOT, including explicit absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDisplayMotSelection {
    pub display: TeletextPageAddress,
    pub mot: Option<TeletextResourceIdentity>,
}

/// Trusted host limits; no producer-deserialized budget or implicit truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDeclaredServiceBudget {
    pub max_mots: usize,
    pub max_displays: usize,
    /// Unique logical resources, also bounding raw companions and DRCS tables.
    pub max_resources: usize,
    /// Every active declaration slot, even repeated, unassociated or suppressed.
    pub max_defaults: usize,
    /// Original catalog definitions, including Local definitions.
    pub max_objects: usize,
}

/// Numeric provenance of a rejected declaration or resolved selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "teletext declarations at display {display:?}, MOT {mot:?}, resource {resource:?}: {reason:?}"
)]
pub struct TeletextDeclaredServiceError {
    pub display: Option<TeletextPageAddress>,
    pub mot: Option<TeletextResourceIdentity>,
    pub resource: Option<TeletextResourceIdentity>,
    pub reason: TeletextDeclaredServiceFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextDeclaredServiceFailure {
    Budget {
        category: &'static str,
        count: usize,
        maximum: usize,
    },
    Size,
    Charset(super::TeletextCharsetValueError),
    UnresolvedPresentation,
    DuplicateMot,
    DuplicateMagazineMot {
        first: TeletextResourceIdentity,
    },
    DuplicateSelection,
    MissingSelection,
    UnknownDisplay,
    MissingMot,
    MotSelection {
        expected: Option<TeletextResourceIdentity>,
    },
    Resolution(TeletextMotResolutionError),
    Unresolved {
        role: TeletextX27LinkRole,
        reason: TeletextMotUnresolvedReason,
    },
    GlobalDefaultConflict {
        slot: u8,
    },
    SelectedRole {
        role: TeletextX27LinkRole,
    },
    ResourceUsage {
        role: TeletextX27LinkRole,
    },
    DefaultSelection {
        slot: u8,
    },
    CompanionKind,
    CompanionOccupied,
    CompanionSelection,
    ResourceCollision,
    MissingResource,
    DefaultDefinition {
        role: TeletextMotLinkRole,
        slot: u8,
    },
}

/// Actual per-display resolution; defaults remain declared even with Local X26.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDeclaredDisplayResolution {
    display: TeletextPageAddress,
    mot: Option<TeletextMotLevel25Resolution>,
    roles: [TeletextMotRoleResolution; 4],
    defaults: [Option<TeletextDefaultObjectSelection>; 4],
}

impl TeletextDeclaredDisplayResolution {
    #[must_use]
    pub const fn display(&self) -> TeletextPageAddress {
        self.display
    }
    #[must_use]
    pub const fn mot(&self) -> Option<&TeletextMotLevel25Resolution> {
        self.mot.as_ref()
    }
    #[must_use]
    pub const fn role(&self, role: TeletextX27LinkRole) -> &TeletextMotRoleResolution {
        &self.roles[role.wire_code() as usize]
    }
    #[must_use]
    pub const fn defaults(&self) -> &[Option<TeletextDefaultObjectSelection>; 4] {
        &self.defaults
    }
    #[must_use]
    pub fn conditional_pop_fallback(&self) -> Option<TeletextMotConditionalPopFallback> {
        self.mot
            .as_ref()
            .and_then(TeletextMotLevel25Resolution::conditional_pop_fallback)
    }
}

/// A real declaration slot requiring an actual resource pointer allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDeclaredDefaultReservation {
    mot: TeletextResourceIdentity,
    role: TeletextMotLinkRole,
    slot: u8,
    selection: TeletextDefaultObjectSelection,
}

impl TeletextDeclaredDefaultReservation {
    #[must_use]
    pub const fn mot(&self) -> TeletextResourceIdentity {
        self.mot
    }
    #[must_use]
    pub const fn role(&self) -> TeletextMotLinkRole {
        self.role
    }
    #[must_use]
    pub const fn slot(&self) -> u8 {
        self.slot
    }
    #[must_use]
    pub const fn selection(&self) -> TeletextDefaultObjectSelection {
        self.selection
    }
}

/// Owned checked correspondence, without a mutable/public authoring capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextDeclaredService {
    semantic: TeletextSemanticService,
    mots: Vec<TeletextMotLevel25Declaration>,
    selections: Vec<TeletextDisplayMotSelection>,
    empty_object_pages: Vec<TeletextResourceIdentity>,
    resolutions: Vec<TeletextDeclaredDisplayResolution>,
    resource_inventory: Vec<TeletextResourceIdentity>,
    defaults: Vec<TeletextDeclaredDefaultReservation>,
}

impl TeletextDeclaredService {
    /// Authenticate the supplied resolved service against the actual declarations.
    /// No semantic input is cloned during validation. All raw counts and derived
    /// storage arithmetic are fenced before allocating indexes or outputs.
    /// # Errors
    /// Rejects budgets, missing/duplicate exact selections, unresolved MOT roles,
    /// contradicted resolved metadata, resource holes/collisions and undefined
    /// defaults. Unproven presentation effects and physical feasibility are not
    /// converted into successful admission.
    pub fn new(
        mut semantic: TeletextSemanticService,
        mots: Vec<TeletextMotLevel25Declaration>,
        selections: Vec<TeletextDisplayMotSelection>,
        empty_object_pages: Vec<TeletextResourceIdentity>,
        budget: TeletextDeclaredServiceBudget,
    ) -> Result<Self, TeletextDeclaredServiceError> {
        let default_count = preflight(&semantic, &mots, &selections, &empty_object_pages, budget)?;
        let mot_index = index_mots(&mots)?;
        let magazine_mots = index_magazine_mots(&mots)?;
        validate_selections(&semantic, &selections, &mot_index, &magazine_mots)?;
        let inventory = inventory(&semantic, &mots, &empty_object_pages, budget.max_resources)?;
        validate_companion_selection(&semantic, &empty_object_pages)?;
        validate_requirements(&semantic, &mots, &inventory)?;
        let mut defaults = Vec::with_capacity(default_count);
        collect_defaults(&semantic, &mots, &mut defaults)?;
        let mut resolutions = Vec::with_capacity(selections.len());
        for selection in &selections {
            let resolution = resolve_display(&semantic, *selection, &mot_index)?;
            if let Some((role, resource, _)) = unannounced_resource_usage(&semantic, &resolution) {
                return Err(TeletextDeclaredServiceError {
                    display: Some(resolution.display),
                    mot: resolution.mot.map(|mot| mot.selected_mot()),
                    resource: Some(resource),
                    ..failure(TeletextDeclaredServiceFailure::ResourceUsage { role })
                });
            }
            resolutions.push(resolution);
        }
        apply_fallbacks(&mut semantic, &resolutions)?;
        Ok(Self {
            semantic,
            mots,
            selections,
            empty_object_pages,
            resolutions,
            resource_inventory: inventory.into_values().collect(),
            defaults,
        })
    }
    #[must_use]
    pub const fn semantic(&self) -> &TeletextSemanticService {
        &self.semantic
    }
    /// Derive exact Unicode Base text changes while retaining all declarations.
    /// The host must authenticate source correspondence and protected effects;
    /// this does not grant concept or publication authority.
    /// # Errors
    /// Rejects empty, duplicate, unbound or stale Text cells and exceeded budgets.
    pub fn with_base_text_edits(
        mut self,
        edits: &[super::TeletextBaseTextEdit],
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextServiceError> {
        self.semantic = self.semantic.with_base_text_edits(edits, budget)?;
        Ok(self)
    }
    /// Replace checked existing local colour/attribute/glyph operands, preserving the
    /// authenticated declarations and resource selections. Hosts must still
    /// enforce captured edit permissions, recompile and publish explicitly.
    /// # Errors
    /// Rejects stale, duplicate or invalid local changes and service budget overflow.
    pub fn with_local_program_operand_overlays(
        mut self,
        overlays: &[super::TeletextLocalProgramOperandOverlay],
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextServiceError> {
        self.semantic = self
            .semantic
            .with_local_program_operand_overlays(overlays, budget)?;
        self.validate_resource_usage()?;
        Ok(self)
    }
    /// Replace exact explicit Main call structure without changing declarations.
    /// # Errors
    /// Rejects stale, duplicate or absent Main programs, invalid targets and budgets.
    pub fn with_main_local_invocation_program_overlays(
        mut self,
        overlays: &[super::TeletextMainLocalInvocationProgramOverlay],
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextServiceError> {
        self.semantic = self
            .semantic
            .with_main_local_invocation_program_overlays(overlays, budget)?;
        self.validate_resource_usage()?;
        Ok(self)
    }
    /// Replace checked glyph/colour/attribute operands in existing local definitions.
    /// Authenticated declarations, object identities/types and resource selections
    /// remain unchanged. Hosts must check all protected effects before publication.
    /// # Errors
    /// Rejects empty, duplicate, absent or stale targets and service budget overflow.
    pub fn with_local_object_operand_overlays(
        mut self,
        overlays: &[super::TeletextLocalObjectOperandOverlay],
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextServiceError> {
        self.semantic = self
            .semantic
            .with_local_object_operand_overlays(overlays, budget)?;
        self.validate_resource_usage()?;
        Ok(self)
    }
    /// Replace exact existing POP/GPOP scalars without rebinding resource owners.
    ///
    /// Preserves MOT/default selections, resource inventory and all declaration
    /// identities. Complete semantic reconstruction checks every consumer and
    /// dependency; hosts must still qualify captured edit permissions and publish
    /// explicitly. This grants no edit or current-output authority.
    /// # Errors
    /// Rejects empty, duplicate, absent or stale targets and budget/dependency failures.
    pub fn with_resource_object_operand_overlays(
        mut self,
        overlays: &[super::TeletextResourceObjectOperandOverlay],
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextServiceError> {
        self.semantic = self
            .semantic
            .with_resource_object_operand_overlays(overlays, budget)?;
        self.validate_resource_usage()?;
        Ok(self)
    }

    /// Replace exact existing POP/GPOP structure while retaining authenticated
    /// MOT/default selections, resource inventory, identities and declarations.
    /// Full semantic reconstruction validates dependencies; host edit permission,
    /// physical packing and explicit publication remain separate gates.
    /// # Errors
    /// Rejects empty, duplicate, absent or stale changes and service/collector limits.
    pub fn with_resource_object_program_overlays(
        mut self,
        overlays: &[super::TeletextResourceObjectProgramOverlay],
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextServiceError> {
        self.semantic = self
            .semantic
            .with_resource_object_program_overlays(overlays, budget)?;
        self.validate_resource_usage()?;
        Ok(self)
    }

    /// Add checked definitions within authenticated existing POP/GPOP resources.
    ///
    /// Existing declarations, inventory, defaults and resolutions remain exact.
    /// Only matching empty-page sentinels disappear when that resource becomes
    /// populated. Full semantic validation grants no edit or publish authority;
    /// hosts must still authenticate intent, effects and native capacity.
    /// # Errors
    /// Rejects empty, duplicate, occupied, absent-resource or invalid additions
    /// and complete service dependency, selection and budget failures.
    pub fn with_resource_object_definition_additions(
        mut self,
        additions: &[super::TeletextResourceObjectDefinitionAddition],
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextResourceObjectDefinitionAdditionApplyError> {
        self.semantic = self
            .semantic
            .with_resource_object_definition_additions(additions, budget)?;
        self.empty_object_pages.retain(|resource| {
            !additions
                .iter()
                .any(|addition| addition.identity().resource() == Some(*resource))
        });
        self.validate_resource_usage()?;
        Ok(self)
    }

    /// Add new complete native object banks without changing MOTs or selections.
    ///
    /// Reconstructs the complete declaration/inventory boundary; semantic success
    /// alone never substitutes for resource, default or MOT validation. Hosts
    /// still need explicit intent/effect/lifecycle checks and native encoding.
    /// # Errors
    /// Rejects occupied/repeated banks, aggregate limits and complete semantic
    /// or declaration validation failures. No partial declaration escapes.
    pub fn with_object_resource_additions(
        self,
        additions: &[super::TeletextObjectResourceAddition],
        service_budget: super::TeletextServiceBudget,
        declaration_budget: TeletextDeclaredServiceBudget,
    ) -> Result<Self, super::TeletextObjectResourceAdditionApplyError> {
        self.with_object_resource_and_definition_additions(
            additions,
            &[],
            service_budget,
            declaration_budget,
        )
    }

    /// Admit new banks and original-bank definitions through one complete graph.
    ///
    /// Preserves MOTs, selections, default provenance and original declarations.
    /// Only populated original companion sentinels are removed; new empty banks
    /// are retained. No intermediate incomplete graph or partial result escapes.
    /// Hosts still authenticate source ownership, lifecycle, effects and publish.
    /// # Errors
    /// Rejects invalid or empty bank batches, non-original definition owners,
    /// complete combined bounds and all semantic/declaration failures.
    pub fn with_object_resource_and_definition_additions(
        self,
        additions: &[super::TeletextObjectResourceAddition],
        definitions: &[super::TeletextResourceObjectDefinitionAddition],
        service_budget: super::TeletextServiceBudget,
        declaration_budget: TeletextDeclaredServiceBudget,
    ) -> Result<Self, super::TeletextObjectResourceAdditionApplyError> {
        // Charge the complete old/new declaration set before semantic cloning,
        // including caller limits smaller than those used for the old service.
        preflight(
            &self.semantic,
            &self.mots,
            &self.selections,
            &self.empty_object_pages,
            declaration_budget,
        )?;
        let resources = self
            .resource_inventory
            .len()
            .checked_add(additions.len())
            .ok_or_else(|| failure(TeletextDeclaredServiceFailure::Size))?;
        fence("resource", resources, declaration_budget.max_resources)?;
        let mut objects = self
            .semantic
            .objects()
            .definitions()
            .len()
            .checked_add(definitions.len())
            .ok_or_else(|| failure(TeletextDeclaredServiceFailure::Size))?;
        fence("object", objects, declaration_budget.max_objects)?;
        for addition in additions {
            objects = objects
                .checked_add(addition.definitions().len())
                .ok_or_else(|| failure(TeletextDeclaredServiceFailure::Size))?;
            fence("object", objects, declaration_budget.max_objects)?;
        }
        let mut owners: BTreeSet<_> = self
            .resource_inventory
            .iter()
            .map(|resource| (resource.page(), resource.logical_subpage()))
            .collect();
        for addition in additions {
            let resource = addition.resource();
            if !owners.insert((resource.page(), resource.logical_subpage())) {
                return Err(super::TeletextObjectResourceAdditionApplyError::Occupied { resource });
            }
        }
        let semantic = self
            .semantic
            .with_object_resource_and_definition_additions(
                additions,
                definitions,
                service_budget,
            )?;
        let empty = semantic.object_companions().to_vec();
        Self::new(
            semantic,
            self.mots,
            self.selections,
            empty,
            declaration_budget,
        )
        .map_err(super::TeletextObjectResourceAdditionApplyError::from)
    }

    /// Replace checked Local glyph programs while retaining all authenticated
    /// declarations, selections, identities, defaults and resource inventory.
    /// Hosts must qualify compiler correspondence and every shared consumer;
    /// this does not grant edit permission or publish the result.
    /// # Errors
    /// Rejects empty, duplicate, absent or stale targets and service/dependency overflow.
    pub fn with_local_object_glyph_program_overlays(
        mut self,
        overlays: &[super::TeletextLocalObjectGlyphProgramOverlay],
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextServiceError> {
        self.semantic = self
            .semantic
            .with_local_object_glyph_program_overlays(overlays, budget)?;
        self.validate_resource_usage()?;
        Ok(self)
    }
    /// Replace exact existing DRCS bitmaps while preserving authenticated MOT,
    /// inventory, defaults and display selections. No edit or publish permission
    /// is granted; shared effects still require full host compilation checks.
    /// # Errors
    /// Rejects stale, duplicate or missing glyph targets and service budget overflow.
    pub fn with_drcs_bitmap_overlays(
        mut self,
        overlays: &[super::TeletextDrcsBitmapOverlay],
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextServiceError> {
        self.semantic = self.semantic.with_drcs_bitmap_overlays(overlays, budget)?;
        Ok(self)
    }
    /// Attach trusted physical rows while preserving the authenticated logical
    /// declarations, resource inventory, MOT selections and exact resolutions.
    /// # Errors
    /// Rejects invalid overlay owners or combined service budget overflow.
    pub fn with_physical_base_overlays(
        mut self,
        overlays: Vec<super::TeletextPhysicalBaseOverlay>,
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextServiceError> {
        self.semantic = self
            .semantic
            .with_physical_base_overlays(overlays, budget)?;
        Ok(self)
    }
    /// Retain explicit native Base Text spelling in the same checked service.
    /// # Errors
    /// Rejects target/scalar/duplicate and combined native/service budget failures.
    pub fn with_base_text_spellings(
        mut self,
        spellings: Vec<super::TeletextBaseTextSpelling>,
        budget: super::TeletextServiceBudget,
    ) -> Result<Self, super::TeletextServiceError> {
        self.semantic = self.semantic.with_base_text_spellings(spellings, budget)?;
        Ok(self)
    }
    fn validate_resource_usage(&self) -> Result<(), super::TeletextServiceError> {
        for resolution in &self.resolutions {
            if let Some((role, resource, source_location)) =
                unannounced_resource_usage(&self.semantic, resolution)
            {
                return Err(super::TeletextServiceError::CompositionalUsage {
                    display: resolution.display,
                    role,
                    page: resource.page(),
                    subpage: resource.logical_subpage(),
                    source_location,
                });
            }
        }
        Ok(())
    }
    #[must_use]
    pub fn mots(&self) -> &[TeletextMotLevel25Declaration] {
        &self.mots
    }
    #[must_use]
    pub fn selections(&self) -> &[TeletextDisplayMotSelection] {
        &self.selections
    }
    #[must_use]
    pub fn empty_object_pages(&self) -> &[TeletextResourceIdentity] {
        &self.empty_object_pages
    }
    #[must_use]
    pub fn resolutions(&self) -> &[TeletextDeclaredDisplayResolution] {
        &self.resolutions
    }
    #[must_use]
    pub fn resource_inventory(&self) -> &[TeletextResourceIdentity] {
        &self.resource_inventory
    }
    #[must_use]
    pub fn defaults(&self) -> &[TeletextDeclaredDefaultReservation] {
        &self.defaults
    }
}

type Inventory = BTreeMap<
    (
        super::TeletextRawDataPageNumber,
        TeletextResourceSubpageIndex,
    ),
    TeletextResourceIdentity,
>;

fn failure(reason: TeletextDeclaredServiceFailure) -> TeletextDeclaredServiceError {
    TeletextDeclaredServiceError {
        display: None,
        mot: None,
        resource: None,
        reason,
    }
}

fn fence(
    category: &'static str,
    count: usize,
    maximum: usize,
) -> Result<(), TeletextDeclaredServiceError> {
    if count > maximum {
        Err(failure(TeletextDeclaredServiceFailure::Budget {
            category,
            count,
            maximum,
        }))
    } else {
        Ok(())
    }
}

fn preflight(
    service: &TeletextSemanticService,
    mots: &[TeletextMotLevel25Declaration],
    selections: &[TeletextDisplayMotSelection],
    companions: &[TeletextResourceIdentity],
    budget: TeletextDeclaredServiceBudget,
) -> Result<usize, TeletextDeclaredServiceError> {
    fence("MOT", mots.len(), budget.max_mots)?;
    fence("display", service.base().pages.len(), budget.max_displays)?;
    fence(
        "metadata",
        service.page_enhancements().len(),
        budget.max_displays,
    )?;
    fence("selection", selections.len(), budget.max_displays)?;
    fence("companion", companions.len(), budget.max_resources)?;
    fence(
        "stored companion",
        service.object_companions().len(),
        budget.max_resources,
    )?;
    fence(
        "DRCS",
        service.drcs().subtables().len(),
        budget.max_resources,
    )?;
    fence(
        "object",
        service.objects().definitions().len(),
        budget.max_objects,
    )?;
    let mut defaults = 0_usize;
    for mot in mots {
        for index in std::iter::once(None).chain(TeletextMotPublicLinkIndex::ALL.map(Some)) {
            for declaration in mot.links().object_link(index).defaults() {
                if matches!(declaration, TeletextMotDefaultObject::Object { .. }) {
                    defaults = defaults
                        .checked_add(1)
                        .ok_or_else(|| failure(TeletextDeclaredServiceFailure::Size))?;
                }
            }
        }
    }
    fence("default", defaults, budget.max_defaults)?;
    // Conservative allocation/work fence: indexes, bounded resolutions and
    // reservations; each MOT has sixteen links and each link at most sixteen S1s.
    let resource_bound = service
        .objects()
        .definitions()
        .len()
        .checked_add(service.drcs().subtables().len())
        .and_then(|n| n.checked_add(mots.len()))
        .and_then(|n| n.checked_add(companions.len()))
        .ok_or_else(|| failure(TeletextDeclaredServiceFailure::Size))?
        .min(budget.max_resources);
    let bytes = mots
        .len()
        .checked_mul(4096)
        .and_then(|n| {
            selections
                .len()
                .checked_mul(4096)
                .and_then(|m| n.checked_add(m))
        })
        .and_then(|n| {
            resource_bound
                .checked_mul(128)
                .and_then(|m| n.checked_add(m))
        })
        .and_then(|n| defaults.checked_mul(128).and_then(|m| n.checked_add(m)))
        .and_then(|n| {
            service
                .base()
                .pages
                .len()
                .checked_mul(128)
                .and_then(|m| n.checked_add(m))
        })
        .and_then(|n| {
            service
                .objects()
                .definitions()
                .len()
                .checked_mul(128)
                .and_then(|m| n.checked_add(m))
        });
    if bytes.is_none_or(|n| isize::try_from(n).is_err()) {
        return Err(failure(TeletextDeclaredServiceFailure::Size));
    }
    Ok(defaults)
}

fn index_mots(
    mots: &[TeletextMotLevel25Declaration],
) -> Result<
    BTreeMap<TeletextResourceIdentity, &TeletextMotLevel25Declaration>,
    TeletextDeclaredServiceError,
> {
    let mut index = BTreeMap::new();
    for mot in mots {
        if index.insert(mot.identity(), mot).is_some() {
            return Err(TeletextDeclaredServiceError {
                mot: Some(mot.identity()),
                ..failure(TeletextDeclaredServiceFailure::DuplicateMot)
            });
        }
    }
    Ok(index)
}

fn validate_selections(
    service: &TeletextSemanticService,
    selections: &[TeletextDisplayMotSelection],
    mots: &BTreeMap<TeletextResourceIdentity, &TeletextMotLevel25Declaration>,
    magazine_mots: &BTreeMap<u8, TeletextResourceIdentity>,
) -> Result<(), TeletextDeclaredServiceError> {
    let owners: BTreeSet<_> = service
        .base()
        .pages
        .iter()
        .map(|page| page.address)
        .collect();
    let mut seen = BTreeSet::new();
    for selection in selections {
        let reason = if !owners.contains(&selection.display) {
            Some(TeletextDeclaredServiceFailure::UnknownDisplay)
        } else if !seen.insert(selection.display) {
            Some(TeletextDeclaredServiceFailure::DuplicateSelection)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(TeletextDeclaredServiceError {
                display: Some(selection.display),
                mot: selection.mot,
                ..failure(reason)
            });
        }
        validate_mot_selection(*selection, mots, magazine_mots)?;
    }
    if let Some(display) = owners.difference(&seen).next() {
        return Err(TeletextDeclaredServiceError {
            display: Some(*display),
            ..failure(TeletextDeclaredServiceFailure::MissingSelection)
        });
    }
    Ok(())
}

fn index_magazine_mots(
    mots: &[TeletextMotLevel25Declaration],
) -> Result<BTreeMap<u8, TeletextResourceIdentity>, TeletextDeclaredServiceError> {
    let mut magazines = BTreeMap::new();
    for declaration in mots {
        let identity = declaration.identity();
        if let Some(first) = magazines.insert(identity.page().magazine(), identity) {
            return Err(TeletextDeclaredServiceError {
                mot: Some(identity),
                ..failure(TeletextDeclaredServiceFailure::DuplicateMagazineMot { first })
            });
        }
    }
    Ok(magazines)
}

fn validate_mot_selection(
    selection: TeletextDisplayMotSelection,
    mots: &BTreeMap<TeletextResourceIdentity, &TeletextMotLevel25Declaration>,
    magazine_mots: &BTreeMap<u8, TeletextResourceIdentity>,
) -> Result<(), TeletextDeclaredServiceError> {
    let location = |reason| TeletextDeclaredServiceError {
        display: Some(selection.display),
        mot: selection.mot,
        ..failure(reason)
    };
    if let Some(identity) = selection.mot {
        if !mots.contains_key(&identity) {
            return Err(location(TeletextDeclaredServiceFailure::MissingMot));
        }
        if identity.page().magazine() != selection.display.page.magazine() {
            return Err(location(TeletextDeclaredServiceFailure::Resolution(
                TeletextMotResolutionError::MagazineOwner {
                    display: selection.display,
                    mot: identity,
                },
            )));
        }
    }
    // EN 300 706 10.6: one MOT applies to a magazine's display-subpage set.
    // A host choice is not an on-wire S1 selector or an opt-out. Annex A.1's
    // recommendation S1=0 does not make an otherwise sole raw owner invalid.
    let expected = magazine_mots
        .get(&selection.display.page.magazine())
        .copied();
    if selection.mot != expected {
        return Err(location(TeletextDeclaredServiceFailure::MotSelection {
            expected,
        }));
    }
    Ok(())
}

fn insert_resource(
    index: &mut Inventory,
    resource: TeletextResourceIdentity,
    repeated: bool,
    maximum: usize,
) -> Result<(), TeletextDeclaredServiceError> {
    let count = index.len();
    match index.entry((resource.page(), resource.logical_subpage())) {
        Entry::Occupied(entry) if repeated && *entry.get() == resource => Ok(()),
        Entry::Occupied(_) => Err(TeletextDeclaredServiceError {
            resource: Some(resource),
            ..failure(TeletextDeclaredServiceFailure::ResourceCollision)
        }),
        Entry::Vacant(entry) => {
            fence("resource", count + 1, maximum)?;
            entry.insert(resource);
            Ok(())
        }
    }
}

fn inventory(
    service: &TeletextSemanticService,
    mots: &[TeletextMotLevel25Declaration],
    companions: &[TeletextResourceIdentity],
    maximum: usize,
) -> Result<Inventory, TeletextDeclaredServiceError> {
    let mut index = BTreeMap::new();
    for definition in service.objects().definitions() {
        if let Some(resource) = definition.identity.resource() {
            insert_resource(&mut index, resource, true, maximum)?;
        }
    }
    for table in service.drcs().subtables() {
        insert_resource(&mut index, table.identity(), false, maximum)?;
    }
    for mot in mots {
        insert_resource(&mut index, mot.identity(), false, maximum)?;
    }
    for resource in companions {
        if !matches!(
            resource.kind(),
            TeletextResourceKind::Pop | TeletextResourceKind::Gpop
        ) {
            return Err(TeletextDeclaredServiceError {
                resource: Some(*resource),
                ..failure(TeletextDeclaredServiceFailure::CompanionKind)
            });
        }
        if index.contains_key(&(resource.page(), resource.logical_subpage())) {
            return Err(TeletextDeclaredServiceError {
                resource: Some(*resource),
                ..failure(TeletextDeclaredServiceFailure::CompanionOccupied)
            });
        }
        insert_resource(&mut index, *resource, false, maximum)?;
    }
    Ok(index)
}

fn require(
    index: &Inventory,
    kind: TeletextResourceKind,
    target: TeletextX27LinkTarget,
    mask: TeletextSubpageRequirementMask,
    display: Option<TeletextPageAddress>,
    mot: Option<TeletextResourceIdentity>,
) -> Result<(), TeletextDeclaredServiceError> {
    let TeletextX27LinkTarget::Page { page } = target else {
        return Ok(());
    };
    for subpage in TeletextResourceSubpageIndex::ALL {
        if mask.requires(subpage)
            && index
                .get(&(page, subpage))
                .is_none_or(|identity| identity.kind() != kind)
        {
            let resource = TeletextResourceIdentity::new(kind, page, subpage).ok();
            return Err(TeletextDeclaredServiceError {
                display,
                mot,
                resource,
                ..failure(TeletextDeclaredServiceFailure::MissingResource)
            });
        }
    }
    Ok(())
}

fn validate_requirements(
    service: &TeletextSemanticService,
    mots: &[TeletextMotLevel25Declaration],
    index: &Inventory,
) -> Result<(), TeletextDeclaredServiceError> {
    for mot in mots {
        for slot in std::iter::once(None).chain(TeletextMotPublicLinkIndex::ALL.map(Some)) {
            let object = mot.links().object_link(slot);
            let drcs = mot.links().drcs_link(slot);
            require(
                index,
                if slot.is_none() {
                    TeletextResourceKind::Gpop
                } else {
                    TeletextResourceKind::Pop
                },
                object.target(),
                object.extent().required_mask(),
                None,
                Some(mot.identity()),
            )?;
            require(
                index,
                if slot.is_none() {
                    TeletextResourceKind::Gdrcs
                } else {
                    TeletextResourceKind::Drcs
                },
                drcs.target(),
                drcs.extent().required_mask(),
                None,
                Some(mot.identity()),
            )?;
        }
    }
    for page in service.page_enhancements().values() {
        if let Some(links) = page.compositional_links {
            for role in TeletextX27LinkRole::ALL {
                let link = links.link(role);
                if link.validity().applies_at_level25() {
                    require(
                        index,
                        role.kind(),
                        link.target(),
                        link.required_subpages(),
                        Some(page.address),
                        None,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn collect_defaults(
    service: &TeletextSemanticService,
    mots: &[TeletextMotLevel25Declaration],
    output: &mut Vec<TeletextDeclaredDefaultReservation>,
) -> Result<(), TeletextDeclaredServiceError> {
    for mot in mots {
        for index in std::iter::once(None).chain(TeletextMotPublicLinkIndex::ALL.map(Some)) {
            let role = index.map_or(TeletextMotLinkRole::Gpop, |index| {
                TeletextMotLinkRole::Pop { index }
            });
            for (slot, declaration) in (0_u8..2).zip(mot.links().object_link(index).defaults()) {
                if let TeletextMotDefaultObject::Object {
                    target,
                    object_type,
                } = declaration
                {
                    if service
                        .objects()
                        .definition(target)
                        .is_none_or(|definition| definition.object_type != object_type)
                    {
                        return Err(TeletextDeclaredServiceError {
                            mot: Some(mot.identity()),
                            resource: target.resource(),
                            ..failure(TeletextDeclaredServiceFailure::DefaultDefinition {
                                role,
                                slot,
                            })
                        });
                    }
                    output.push(TeletextDeclaredDefaultReservation {
                        mot: mot.identity(),
                        role,
                        slot,
                        selection: TeletextDefaultObjectSelection {
                            target,
                            object_type,
                        },
                    });
                }
            }
        }
    }
    Ok(())
}

fn no_mot_role(
    links: Option<TeletextX27Level25Links>,
    display: TeletextPageAddress,
    role: TeletextX27LinkRole,
) -> TeletextMotRoleResolution {
    let Some(link) = links
        .map(|links| links.link(role))
        .filter(|link| link.validity().applies_at_level25())
    else {
        return TeletextMotRoleResolution::Unselected;
    };
    match link.target() {
        TeletextX27LinkTarget::Unspecified { .. } => TeletextMotRoleResolution::Unselected,
        TeletextX27LinkTarget::Page { page } => TeletextMotRoleResolution::Selected {
            target: page,
            requirements: super::TeletextMotResolutionRequirements::X27Mask(
                link.required_subpages(),
            ),
            provenance: TeletextMotLinkProvenance::X27 { display, role },
            original_mot: None,
        },
    }
}

fn resolve_display(
    service: &TeletextSemanticService,
    selection: TeletextDisplayMotSelection,
    mots: &BTreeMap<TeletextResourceIdentity, &TeletextMotLevel25Declaration>,
) -> Result<TeletextDeclaredDisplayResolution, TeletextDeclaredServiceError> {
    let metadata = service.page_enhancements().get(&selection.display);
    let links = metadata.and_then(|page| page.compositional_links);
    let mut result = TeletextDeclaredDisplayResolution {
        display: selection.display,
        mot: None,
        roles: TeletextX27LinkRole::ALL.map(|role| no_mot_role(links, selection.display, role)),
        defaults: [None; 4],
    };
    if let Some(identity) = selection.mot {
        let mot = mots
            .get(&identity)
            .ok_or_else(|| TeletextDeclaredServiceError {
                display: Some(selection.display),
                mot: Some(identity),
                ..failure(TeletextDeclaredServiceFailure::MissingMot)
            })?;
        let resolved = TeletextMotLevel25Resolution::resolve(mot, selection.display, links)
            .map_err(|error| TeletextDeclaredServiceError {
                display: Some(selection.display),
                mot: Some(identity),
                ..failure(TeletextDeclaredServiceFailure::Resolution(error))
            })?;
        if let Some(conflict) = resolved.default_conflicts().iter().flatten().next() {
            return Err(TeletextDeclaredServiceError {
                display: Some(selection.display),
                mot: Some(identity),
                ..failure(TeletextDeclaredServiceFailure::GlobalDefaultConflict {
                    slot: conflict.slot,
                })
            });
        }
        result.roles = TeletextX27LinkRole::ALL.map(|role| *resolved.role(role));
        for (bank, source) in resolved.default_sources().iter().enumerate() {
            if let Some(source) = source {
                for (slot, declaration) in source.link.defaults().into_iter().enumerate() {
                    if let TeletextMotDefaultObject::Object {
                        target,
                        object_type,
                    } = declaration
                    {
                        result.defaults[bank * 2 + slot] = Some(TeletextDefaultObjectSelection {
                            target,
                            object_type,
                        });
                    }
                }
            }
        }
        result.mot = Some(resolved);
    }
    compare_selection(&result, metadata.map(|page| page.resources))?;
    Ok(result)
}

fn compare_selection(
    result: &TeletextDeclaredDisplayResolution,
    supplied: Option<TeletextResolvedPageResourceSelection>,
) -> Result<(), TeletextDeclaredServiceError> {
    for role in TeletextX27LinkRole::ALL {
        let expected = match result.role(role) {
            TeletextMotRoleResolution::Unselected => None,
            TeletextMotRoleResolution::Selected { target, .. } => Some(*target),
            TeletextMotRoleResolution::Unresolved { reason, .. } => {
                return Err(TeletextDeclaredServiceError {
                    display: Some(result.display),
                    mot: result.mot.map(|mot| mot.selected_mot()),
                    ..failure(TeletextDeclaredServiceFailure::Unresolved {
                        role,
                        reason: *reason,
                    })
                })
            }
        };
        let actual = supplied.and_then(|resources| match role {
            TeletextX27LinkRole::Gpop => resources.objects.global,
            TeletextX27LinkRole::Pop => resources.objects.public,
            TeletextX27LinkRole::Gdrcs => {
                resources.drcs.page(super::TeletextDrcsTableScope::Global)
            }
            TeletextX27LinkRole::Drcs => resources.drcs.page(super::TeletextDrcsTableScope::Normal),
        });
        if actual != expected {
            return Err(TeletextDeclaredServiceError {
                display: Some(result.display),
                mot: result.mot.map(|mot| mot.selected_mot()),
                ..failure(TeletextDeclaredServiceFailure::SelectedRole { role })
            });
        }
    }
    let actual = supplied.map_or([None; 4], |resources| resources.defaults);
    for (slot, (expected, actual)) in (0_u8..4).zip(result.defaults.into_iter().zip(actual)) {
        if expected != actual {
            return Err(TeletextDeclaredServiceError {
                display: Some(result.display),
                mot: result.mot.map(|mot| mot.selected_mot()),
                ..failure(TeletextDeclaredServiceFailure::DefaultSelection { slot })
            });
        }
    }
    Ok(())
}

/// EN 300 706 10.5.1.4, 10.5.2.2 and Tables 21/23: absent an
/// applicable X27 override, the selected MOT extent must announce each used S1.
/// Dormant extra banks are not consumers. Original synthesized default roots
/// retain their own MOT provenance; their original declaration already checks
/// their extent, even when the effective role is superseded by another owner.
fn unannounced_resource_usage(
    service: &TeletextSemanticService,
    resolution: &TeletextDeclaredDisplayResolution,
) -> Option<(
    TeletextX27LinkRole,
    TeletextResourceIdentity,
    super::TeletextEnhancementSource,
)> {
    let (program_source, references) = service.collected(resolution.display)?;
    let check = |role, resource: TeletextResourceIdentity, source| {
        if let TeletextMotRoleResolution::Selected {
            target,
            requirements: super::TeletextMotResolutionRequirements::MotExtent(extent),
            ..
        } = resolution.role(role)
        {
            if *target == resource.page() && !extent.requires(resource.logical_subpage()) {
                return Some((role, resource, source));
            }
        }
        None
    };
    for invocation in references.object_references() {
        if matches!(
            program_source,
            super::TeletextEffectiveProgramSource::Defaults { .. }
        ) && invocation.source_location.object.is_none()
        {
            continue;
        }
        if let Some(resource) = invocation.target.resource() {
            let role = match resource.kind() {
                TeletextResourceKind::Pop => TeletextX27LinkRole::Pop,
                TeletextResourceKind::Gpop => TeletextX27LinkRole::Gpop,
                _ => continue,
            };
            if let Some(failure) = check(role, resource, invocation.source_location) {
                return Some(failure);
            }
        }
    }
    let page = service.page_enhancements().get(&resolution.display)?;
    for (reference, source) in references.references().iter().zip(references.sources()) {
        let target = page.resources.drcs.page(reference.table)?;
        let role = match reference.table {
            super::TeletextDrcsTableScope::Normal => TeletextX27LinkRole::Drcs,
            super::TeletextDrcsTableScope::Global => TeletextX27LinkRole::Gdrcs,
        };
        let resource =
            TeletextResourceIdentity::new(role.kind(), target, reference.subtable.into()).ok()?;
        if let Some(failure) = check(role, resource, *source) {
            return Some(failure);
        }
    }
    None
}

fn apply_fallbacks(
    service: &mut TeletextSemanticService,
    resolutions: &[TeletextDeclaredDisplayResolution],
) -> Result<(), TeletextDeclaredServiceError> {
    service.clear_mot_fallbacks();
    let by_display: BTreeMap<_, _> = resolutions
        .iter()
        .map(|value| (value.display, value))
        .collect();
    let mut pending = Vec::with_capacity(resolutions.len());
    for page in &service.base().pages {
        if service
            .page_enhancements()
            .get(&page.address)
            .is_some_and(|metadata| metadata.presentation.is_some())
            || service
                .magazine_presentations()
                .contains_key(&page.address.page.magazine())
        {
            continue;
        }
        let Some(resolution) = by_display.get(&page.address) else {
            continue;
        };
        let Some(source) = resolution.mot().and_then(|mot| mot.default_sources()[1]) else {
            continue;
        };
        let flags = source.link.raw_fallback();
        let location = || TeletextDeclaredServiceError {
            display: Some(page.address),
            mot: resolution
                .mot()
                .map(TeletextMotLevel25Resolution::selected_mot),
            ..failure(TeletextDeclaredServiceFailure::UnresolvedPresentation)
        };
        // FF preserves raw conditional flags; no primary proof establishes their
        // nontrivial effects for an unused link. An actual X28/M29 overrides them.
        if matches!(
            source.link.target(),
            TeletextX27LinkTarget::Unspecified { .. }
        ) && !flags.protocol_no_panels()
            && (flags.panel_code() != 0 || flags.black_background_substitution())
        {
            return Err(location());
        }
        let defaults =
            super::resolve_teletext_mot_presentation_fallback(page.national_option, flags)
                .map_err(|error| TeletextDeclaredServiceError {
                    reason: TeletextDeclaredServiceFailure::Charset(error),
                    ..location()
                })?;
        pending.push((page.address, defaults));
    }
    for (owner, defaults) in pending {
        service.set_mot_fallback(owner, defaults);
    }
    Ok(())
}

fn validate_companion_selection(
    service: &TeletextSemanticService,
    supplied: &[TeletextResourceIdentity],
) -> Result<(), TeletextDeclaredServiceError> {
    let original: BTreeSet<_> = service.object_companions().iter().copied().collect();
    let actual: BTreeSet<_> = supplied.iter().copied().collect();
    if original != actual {
        return Err(failure(TeletextDeclaredServiceFailure::CompanionSelection));
    }
    Ok(())
}
