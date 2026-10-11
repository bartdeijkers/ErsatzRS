//! One producer-derived Passive recipe, not an augmented program or admission.
//!
//! The exact display-origin (including repaint lineage) stays borrowed from the
//! original composition. No glyph is selected by equal font masks. Placement,
//! component order, final packing and whole-plane equivalence remain separate.

use super::TeletextMainComposition;
use ersatzrs_domain::{
    resolve_teletext_national_conversion, TeletextCharsetSelection, TeletextColourCompositionError,
    TeletextColourPixelOptions, TeletextColumnOperation, TeletextDisplayAttributes,
    TeletextDisplayOrigin, TeletextDisplaySize, TeletextEnhancementCharsetDesignation,
    TeletextEnhancementCommand, TeletextEnhancementObjectCatalog,
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram,
    TeletextEnhancementProgramError, TeletextEnhancementSource, TeletextExecutedGlyphIntent,
    TeletextFlashMode, TeletextFlashPhase, TeletextFlashRate, TeletextGlyphCandidate,
    TeletextGlyphCode, TeletextGlyphDisposition, TeletextGlyphIdentity,
    TeletextObjectCatalogBudget, TeletextObjectId, TeletextObjectIdentity, TeletextObjectType,
    TeletextPageAddress, TeletextSpatialInvocationId, TeletextSpatialRootSource,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextOriginCarrierBudget {
    pub max_candidates: usize,
    pub max_origins: usize,
    pub catalog: TeletextObjectCatalogBudget,
    /// Catalogue entries scanned plus probed exact u16 IDs.
    pub max_id_checks: usize,
    pub max_output_commands: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextOriginCarrierCommandRole {
    Foreground,
    Background,
    Flash,
    Display,
    Charset,
    Glyph,
}
/// Every role is derived from this same original origin; callers cannot supply
/// a detached source map. Definition coordinates are relative zero, awaiting a
/// separately proved caller anchor/offset.
#[derive(Debug)]
pub struct TeletextOriginCarrier<'a> {
    composition: &'a TeletextMainComposition,
    catalog: &'a TeletextEnhancementObjectCatalog,
    origin_index: usize,
    candidate: TeletextGlyphCandidate,
    definition: TeletextEnhancementObjectDefinition,
    roles: Vec<TeletextOriginCarrierCommandRole>,
}
impl TeletextOriginCarrier<'_> {
    #[must_use]
    pub const fn composition(&self) -> &TeletextMainComposition {
        self.composition
    }
    #[must_use]
    pub const fn catalog(&self) -> &TeletextEnhancementObjectCatalog {
        self.catalog
    }
    #[must_use]
    pub const fn original_origin_index(&self) -> usize {
        self.origin_index
    }
    #[must_use]
    pub fn original_origin(&self) -> &TeletextDisplayOrigin {
        &self.composition.display_resolution().origins()[self.origin_index]
    }
    #[must_use]
    pub fn candidate_index(&self) -> usize {
        self.original_origin().candidate_index()
    }
    #[must_use]
    pub const fn candidate(&self) -> &TeletextGlyphCandidate {
        &self.candidate
    }
    #[must_use]
    pub const fn definition(&self) -> &TeletextEnhancementObjectDefinition {
        &self.definition
    }
    #[must_use]
    pub fn command_roles(&self) -> &[TeletextOriginCarrierCommandRole] {
        &self.roles
    }
    #[must_use]
    pub fn location(&self) -> Option<TeletextEnhancementSource> {
        self.candidate.source()
    }
    #[must_use]
    pub fn root_source(&self) -> Option<TeletextSpatialRootSource> {
        self.candidate.root_source()
    }
    #[must_use]
    pub fn invocation(&self) -> Option<TeletextSpatialInvocationId> {
        self.candidate.invocation()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextOriginCarrierFailure {
    #[error("{kind} count {count} exceeds trusted budget {maximum}")]
    InputBudget {
        kind: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("bounded carrier storage/work exceeds supported range")]
    AllocationSize,
    #[error("origin/candidate correspondence is absent")]
    Correspondence,
    #[error("original geometry is unresolved")]
    Geometry,
    #[error("original origin remains pending at {index}")]
    OriginalPending { index: usize },
    #[error("original {field} attribute remains unresolved")]
    Attribute { field: &'static str },
    #[error("original glyph assignment remains unresolved")]
    Glyph,
    #[error("original charset remains unresolved")]
    Charset,
    #[error("Base national-subset identity needs explicit enhancement conversion")]
    NationalConversion,
    #[error("incremental/decremental flash needs original effective temporal phase")]
    TemporalFlash,
    #[error("no final owned fragment proves original colour dependencies")]
    NoOwnedFragment,
    #[error("local identity namespace is exhausted")]
    IdentityExhausted,
    #[error(transparent)]
    Raster(Box<super::TeletextRasterCellFailure>),
    #[error("undefined glyph response cannot preserve raw spacing separation in a fresh carrier")]
    UndefinedSpacing,
    #[error(transparent)]
    Colour(Box<TeletextColourCompositionError>),
    #[error(transparent)]
    Program(#[from] TeletextEnhancementProgramError),
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext origin carrier for {owner:?}, origin {origin}: {reason}")]
pub struct TeletextOriginCarrierError {
    pub owner: TeletextPageAddress,
    pub origin: usize,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextOriginCarrierFailure,
}

/// Derive exactly one fresh Passive definition from known original evidence.
/// Raw input/storage limits precede scans/allocation. The complete body count is
/// checked before allocation; original inputs and catalogue are never mutated.
/// Unknown conversion/temporal states are located derivation gaps, not authored
/// feature restrictions. The recipe itself establishes no placement or readiness.
/// # Errors
/// Preserves original uncertainty and colour-parameter failures; rejects exceeded
/// budgets, missing correspondence, unsafe geometry and identity exhaustion.
pub fn build_teletext_origin_carrier<'a>(
    composition: &'a TeletextMainComposition,
    origin_index: usize,
    catalog: &'a TeletextEnhancementObjectCatalog,
    budget: TeletextOriginCarrierBudget,
) -> Result<TeletextOriginCarrier<'a>, TeletextOriginCarrierError> {
    let fail = |reason| located(composition, origin_index, reason);
    preflight(composition, catalog, budget).map_err(fail)?;
    let recipe = derive_recipe(composition, origin_index).map_err(fail)?;
    let count = 5 + usize::from(recipe.charset.is_some());
    bound("output commands", count, budget.max_output_commands).map_err(fail)?;
    let identity =
        free_identity(composition.owner(), catalog, budget.max_id_checks).map_err(fail)?;
    let mut commands = Vec::with_capacity(count);
    let mut roles = Vec::with_capacity(count);
    let tail = recipe.charset.map(|designation| {
        (
            Role::Charset,
            TeletextColumnOperation::DesignateCharset { designation },
        )
    });
    for (role, operation) in recipe
        .prefix
        .into_iter()
        .chain(tail)
        .chain([(Role::Glyph, recipe.glyph)])
    {
        roles.push(role);
        commands.push(TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation,
        });
    }
    let definition = TeletextEnhancementObjectDefinition {
        identity,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(commands).map_err(|error| fail(error.into()))?,
    };
    Ok(TeletextOriginCarrier {
        composition,
        catalog,
        origin_index,
        candidate: recipe.candidate,
        definition,
        roles,
    })
}

use TeletextOriginCarrierCommandRole as Role;
struct Recipe {
    candidate: TeletextGlyphCandidate,
    prefix: [(Role, TeletextColumnOperation); 4],
    charset: Option<TeletextEnhancementCharsetDesignation>,
    glyph: TeletextColumnOperation,
}
fn located(
    composition: &TeletextMainComposition,
    origin: usize,
    reason: Failure,
) -> TeletextOriginCarrierError {
    let candidate = composition
        .display_resolution()
        .origins()
        .get(origin)
        .and_then(|origin| {
            composition
                .glyph_layers()
                .candidates()
                .get(origin.candidate_index())
        });
    TeletextOriginCarrierError {
        owner: composition.owner(),
        origin,
        location: candidate.and_then(TeletextGlyphCandidate::source),
        root_source: candidate.and_then(TeletextGlyphCandidate::root_source),
        invocation: candidate.and_then(TeletextGlyphCandidate::invocation),
        reason,
    }
}
fn derive_recipe(
    composition: &TeletextMainComposition,
    origin_index: usize,
) -> Result<Recipe, Failure> {
    let display = composition.display_resolution();
    let origin = display
        .origins()
        .get(origin_index)
        .ok_or(Failure::Correspondence)?;
    let candidate = composition
        .glyph_layers()
        .candidates()
        .get(origin.candidate_index())
        .copied()
        .ok_or(Failure::Correspondence)?;
    if let Some(index) = origin.pending() {
        return Err(Failure::OriginalPending { index });
    }
    let size = origin.effective_size().ok_or(Failure::Geometry)?;
    let position = origin.visual_position().ok_or(Failure::Geometry)?;
    let attrs = display
        .origin_attributes(origin_index)
        .ok_or(Failure::Correspondence)?;
    let d5 = attrs
        .underline_or_separated_mosaics()
        .value()
        .ok_or(Failure::Attribute { field: "D5" })?;
    let intent = composition
        .glyph_intent()
        .records()
        .get(origin.candidate_index())
        .filter(|record| *record.context().candidate() == candidate)
        .ok_or(Failure::Correspondence)?;
    let (glyph, charset) = glyph_recipe(composition, intent)?;
    let separation = if candidate.base().is_some()
        && matches!(intent.glyph(), Some(TeletextGlyphIdentity::G1Mosaic { .. }))
    {
        d5 || attrs
            .spacing_separated()
            .value()
            .ok_or(Failure::Attribute {
                field: "spacing separation",
            })?
    } else {
        d5
    };
    if matches!(
        intent.glyph(),
        Some(TeletextGlyphIdentity::G3 { .. } | TeletextGlyphIdentity::Drcs { .. })
    ) && attrs.spacing_separated().value() != Some(false)
    {
        return Err(Failure::UndefinedSpacing);
    }
    let prefix = recipe_prefix(attrs, size, separation)?;
    validate_colour(composition, origin_index, position, size)?;
    Ok(Recipe {
        candidate,
        prefix,
        charset,
        glyph,
    })
}
fn recipe_prefix(
    attrs: TeletextDisplayAttributes<'_>,
    size: TeletextDisplaySize,
    d5: bool,
) -> Result<[(Role, TeletextColumnOperation); 4], Failure> {
    let known = |field| Failure::Attribute { field };
    let foreground = attrs
        .foreground()
        .value()
        .ok_or_else(|| known("foreground"))?;
    let background = attrs
        .background()
        .value()
        .ok_or_else(|| known("background"))?;
    let flash = attrs.flash().value().ok_or_else(|| known("flash"))?;
    if flash.mode() != TeletextFlashMode::Steady
        && matches!(
            flash.rate(),
            TeletextFlashRate::Fast2HzIncremental | TeletextFlashRate::Fast2HzDecremental
        )
    {
        return Err(Failure::TemporalFlash);
    }
    let boxed = attrs.boxed().value().ok_or_else(|| known("boxed"))?;
    let conceal = attrs.conceal().value().ok_or_else(|| known("conceal"))?;
    let invert = attrs.invert().value().ok_or_else(|| known("invert"))?;
    Ok([
        (
            Role::Foreground,
            TeletextColumnOperation::Foreground { colour: foreground },
        ),
        (
            Role::Background,
            TeletextColumnOperation::Background { colour: background },
        ),
        (
            Role::Flash,
            TeletextColumnOperation::Flash {
                mode: flash.mode(),
                rate: flash.rate(),
            },
        ),
        (
            Role::Display,
            TeletextColumnOperation::Attributes {
                double_width: matches!(
                    size,
                    TeletextDisplaySize::DoubleWidth | TeletextDisplaySize::DoubleSize
                ),
                double_height: matches!(
                    size,
                    TeletextDisplaySize::DoubleHeight | TeletextDisplaySize::DoubleSize
                ),
                boxed,
                conceal,
                invert,
                underline_or_separated_mosaics: d5,
            },
        ),
    ])
}

type Failure = TeletextOriginCarrierFailure;
fn bound(kind: &'static str, count: usize, maximum: usize) -> Result<(), Failure> {
    if count > maximum {
        Err(Failure::InputBudget {
            kind,
            count,
            maximum,
        })
    } else {
        Ok(())
    }
}
fn preflight(
    composition: &TeletextMainComposition,
    catalog: &TeletextEnhancementObjectCatalog,
    budget: TeletextOriginCarrierBudget,
) -> Result<(), Failure> {
    bound(
        "candidates",
        composition.glyph_layers().candidates().len(),
        budget.max_candidates,
    )?;
    bound(
        "origins",
        composition.display_resolution().origins().len(),
        budget.max_origins,
    )?;
    bound(
        "catalogue objects",
        catalog.definitions().len(),
        budget.catalog.max_objects,
    )?;
    for (count, size) in [
        (
            catalog.definitions().len(),
            std::mem::size_of::<TeletextEnhancementObjectDefinition>(),
        ),
        (6, std::mem::size_of::<TeletextEnhancementCommand>()),
        (6, std::mem::size_of::<TeletextOriginCarrierCommandRole>()),
    ] {
        if count
            .checked_mul(size)
            .is_none_or(|bytes| isize::try_from(bytes).is_err())
        {
            return Err(Failure::AllocationSize);
        }
    }
    let mut count = 0usize;
    for definition in catalog.definitions() {
        count = count
            .checked_add(definition.program.commands().len())
            .ok_or(Failure::AllocationSize)?;
        bound("catalogue commands", count, budget.catalog.max_commands)?;
    }
    Ok(())
}
fn free_identity(
    owner: TeletextPageAddress,
    catalog: &TeletextEnhancementObjectCatalog,
    maximum: usize,
) -> Result<TeletextObjectIdentity, Failure> {
    // Finite exact u16 namespace: 1024 words = 8 KiB, no reserved ID range.
    bound("identity checks", catalog.definitions().len(), maximum)?;
    let mut occupied = [0_u64; 1024];
    for definition in catalog.definitions() {
        if definition.identity.owner() == Some(owner) {
            let id = usize::from(definition.identity.object_id().get());
            occupied[id / 64] |= 1 << (id % 64);
        }
    }
    for id in 0..=u16::MAX {
        let checks = catalog
            .definitions()
            .len()
            .checked_add(usize::from(id) + 1)
            .ok_or(Failure::AllocationSize)?;
        bound("identity checks", checks, maximum)?;
        if occupied[usize::from(id) / 64] & (1 << (usize::from(id) % 64)) == 0 {
            return Ok(TeletextObjectIdentity::local(
                owner,
                TeletextObjectId::new(id),
            ));
        }
    }
    Err(Failure::IdentityExhausted)
}
fn glyph_recipe(
    composition: &TeletextMainComposition,
    intent: &TeletextExecutedGlyphIntent,
) -> Result<
    (
        TeletextColumnOperation,
        Option<TeletextEnhancementCharsetDesignation>,
    ),
    Failure,
> {
    let candidate = intent.context().candidate();
    if let Some(base) = candidate.base() {
        if let Some(operation) =
            composition.base_replacement(base.position().row(), base.position().column())
        {
            return Ok((
                operation,
                Some(TeletextEnhancementCharsetDesignation::new(0).map_err(|_| Failure::Charset)?),
            ));
        }
    }
    if intent.disposition() != TeletextGlyphDisposition::Assigned {
        return Err(Failure::Glyph);
    }
    let identity = intent.glyph().ok_or(Failure::Glyph)?;
    if matches!(identity, TeletextGlyphIdentity::G0 { glyph } if glyph.national_subset().is_some())
    {
        let conversion = resolve_teletext_national_conversion(identity)
            .map_err(|_| Failure::NationalConversion)?;
        return Ok((conversion.operation(), Some(conversion.designation())));
    }
    // Mode 10 / code2A is @ independently of even unresolved current G0.
    if identity == TeletextGlyphIdentity::AtSymbol {
        return candidate
            .operation()
            .map(|operation| (operation, None))
            .ok_or(Failure::Correspondence);
    }
    let selection = intent.context().selection();
    let charset = selection
        .map(
            |selection| match selection.value().ok_or(Failure::Charset)? {
                TeletextCharsetSelection::Enhancement(value) => Ok(value),
                TeletextCharsetSelection::PagePrimary(value) => {
                    TeletextEnhancementCharsetDesignation::new(value.get())
                        .map_err(|_| Failure::Charset)
                }
            },
        )
        .transpose()?;
    let operation = if let Some(operation) = candidate.operation() {
        operation
    } else {
        match identity {
            TeletextGlyphIdentity::G0 { glyph } => TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(glyph.code()).map_err(|_| Failure::Glyph)?,
            },
            TeletextGlyphIdentity::G1Mosaic { mosaic } => TeletextColumnOperation::G1 { mosaic },
            TeletextGlyphIdentity::Blank | TeletextGlyphIdentity::SpacingControl { .. } => {
                TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(0x20).map_err(|_| Failure::Glyph)?,
                }
            }
            _ => return Err(Failure::Glyph),
        }
    };
    Ok((operation, charset))
}
fn validate_colour(
    composition: &TeletextMainComposition,
    origin: usize,
    (row, column): (u8, u8),
    size: TeletextDisplaySize,
) -> Result<(), Failure> {
    let width = if matches!(
        size,
        TeletextDisplaySize::DoubleWidth | TeletextDisplaySize::DoubleSize
    ) {
        2
    } else {
        1
    };
    let height = if matches!(
        size,
        TeletextDisplaySize::DoubleHeight | TeletextDisplaySize::DoubleSize
    ) {
        2
    } else {
        1
    };
    let display = composition.display_resolution();
    let left = composition.display_geometry().left_width();
    let area = |column| {
        if column < left {
            0
        } else if column < left + 40 {
            1
        } else {
            2
        }
    };
    if row + height > 25
        || column + width > display.width()
        || area(column) != area(column + width - 1)
    {
        return Err(Failure::Geometry);
    }
    let mut owned = false;
    for part_y in 0..height {
        for part_x in 0..width {
            let cell = display
                .cell(row + part_y, column + part_x)
                .ok_or(Failure::Geometry)?;
            if cell.origin() != Some(origin) {
                continue;
            }
            owned = true;
            let glyph = super::raster::comparison_original_glyph(composition, cell)
                .map_err(|error| Failure::Raster(Box::new(error)))?;
            for y in 0..10 {
                for x in 0..12 {
                    let (source_x, source_y) =
                        super::raster::fragment_position(cell.fragment(), x, y);
                    let covered = if let Some(mask) = glyph.mask {
                        mask.pixel(source_x, source_y).ok_or(Failure::Geometry)?
                    } else {
                        false
                    };
                    for reveal_concealed in [false, true] {
                        for flash_phase in
                            [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate]
                        {
                            composition
                                .colour_composition()
                                .pixel(
                                    display,
                                    row + part_y,
                                    column + part_x,
                                    covered,
                                    TeletextColourPixelOptions {
                                        reveal_concealed,
                                        flash_phase,
                                    },
                                )
                                .map_err(|error| Failure::Colour(Box::new(error)))?;
                        }
                    }
                }
            }
        }
    }
    if owned {
        Ok(())
    } else {
        Err(Failure::NoOwnedFragment)
    }
}
