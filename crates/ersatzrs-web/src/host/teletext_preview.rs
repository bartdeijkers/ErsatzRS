//! Exact-address native preview from a complete compiler-owned service.
//! This is neither a base fallback view nor browser/client/output admission.

use std::fmt;

use ersatzrs_domain::{TeletextColourPixelOptions, TeletextNavigationLink, TeletextPageAddress};
use ersatzrs_ffmpeg::teletext::{
    render_teletext_composition, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextMainComposition, TeletextRaster, TeletextRasterBudget, TeletextRasterError,
};

/// Trusted host bounds, independent of producer content and invocation limits.
#[derive(Debug, Clone, Copy)]
pub struct TeletextPreviewBudget {
    pub max_pages: usize,
    pub max_model_records: usize,
    pub max_pixels: usize,
    pub max_diagnostics: usize,
    /// Conservative record/cell/pixel work units, not a wall-clock deadline.
    pub max_work: usize,
}

/// Enhanced selected model and exact typed navigation from the same service.
/// The borrowed navigation cannot be associated with a foreign model by callers.
/// RGB4, video, receiver header and unresolved pixels remain distinct raster roles.
pub struct TeletextNativePreview<'a> {
    raster: TeletextRaster,
    navigation: &'a [TeletextNavigationLink],
}
impl<'a> TeletextNativePreview<'a> {
    #[must_use]
    pub const fn raster(&self) -> &TeletextRaster {
        &self.raster
    }
    #[must_use]
    pub const fn navigation(&self) -> &[TeletextNavigationLink] {
        self.navigation
    }

    /// Move the bounded raster without copying its pixels; navigation remains
    /// borrowed from the exact validated service that produced this preview.
    #[must_use]
    pub fn into_parts(self) -> (TeletextRaster, &'a [TeletextNavigationLink]) {
        (self.raster, self.navigation)
    }
}
impl fmt::Debug for TeletextNativePreview<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextNativePreview")
            .field("owner", &self.raster.owner())
            .field("width", &self.raster.width())
            .field("diagnostics", &self.raster.diagnostics().len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("teletext preview {owner:?}: {reason}")]
pub struct TeletextPreviewError {
    pub owner: TeletextPageAddress,
    pub reason: TeletextPreviewFailure,
}
#[derive(Debug, thiserror::Error)]
pub enum TeletextPreviewFailure {
    #[error("the exact display page/subcode is absent")]
    MissingDisplay,
    #[error("{category} count {count} exceeds host maximum {maximum}")]
    Budget {
        category: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("preview size/work arithmetic overflowed")]
    Size,
    #[error(transparent)]
    Raster(#[from] TeletextRasterError),
}

/// Project one exact display without a first-page or first-subcode fallback.
/// All count/work fences precede raster allocation. The canonical renderer
/// checks pixel/diagnostic storage and returns no partial raster on failure.
/// Async hosts must perform this bounded synchronous work in a joined worker.
/// Navigation comes from original checked display intent, not generated glyphs.
/// # Errors
/// Returns an exact-owner error for missing display, budget/arithmetic or raster
/// failures. Unresolved cells remain located diagnostics in a successful raster.
pub fn project_teletext_preview(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    options: TeletextColourPixelOptions,
    budget: TeletextPreviewBudget,
) -> Result<TeletextNativePreview<'_>, TeletextPreviewError> {
    let error = |reason| TeletextPreviewError { owner, reason };
    let pages = service.pages();
    let base = &service.declared().semantic().base().pages;
    limit(pages.len(), budget.max_pages, "encoded pages").map_err(error)?;
    limit(base.len(), budget.max_pages, "display pages").map_err(error)?;
    let scans = pages
        .len()
        .checked_add(base.len())
        .ok_or_else(|| error(TeletextPreviewFailure::Size))?;
    limit(scans, budget.max_work, "work").map_err(error)?;
    let composition = pages
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::NativeDisplay(page)
                if page.composition().owner() == owner =>
            {
                Some(page.composition())
            }
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == owner =>
            {
                Some(page.normalized().original())
            }
            _ => None,
        })
        .ok_or_else(|| error(TeletextPreviewFailure::MissingDisplay))?;
    let page = base
        .iter()
        .find(|page| page.address == owner)
        .ok_or_else(|| error(TeletextPreviewFailure::MissingDisplay))?;
    preflight(composition, pages.len(), base.len(), budget).map_err(error)?;
    let raster = render_teletext_composition(
        composition,
        options,
        TeletextRasterBudget {
            max_pixels: budget.max_pixels,
            max_diagnostics: budget.max_diagnostics,
        },
    )
    .map_err(|reason| error(TeletextPreviewFailure::Raster(reason)))?;
    Ok(TeletextNativePreview {
        raster,
        navigation: &page.navigation,
    })
}

fn limit(
    count: usize,
    maximum: usize,
    category: &'static str,
) -> Result<(), TeletextPreviewFailure> {
    if count > maximum {
        Err(TeletextPreviewFailure::Budget {
            category,
            count,
            maximum,
        })
    } else {
        Ok(())
    }
}

fn preflight(
    composition: &TeletextMainComposition,
    pages: usize,
    base_pages: usize,
    budget: TeletextPreviewBudget,
) -> Result<(), TeletextPreviewFailure> {
    let display = composition.display_resolution();
    let records = [
        composition.spatial_trace().events().len(),
        composition.glyph_intent().records().len(),
        composition.drcs_execution().records().len(),
        display.origins().len(),
        display.writers().len(),
        display.pending().len(),
    ]
    .into_iter()
    .try_fold(0usize, usize::checked_add)
    .ok_or(TeletextPreviewFailure::Size)?;
    limit(records, budget.max_model_records, "model records")?;
    let cells = usize::from(display.width())
        .checked_mul(25)
        .ok_or(TeletextPreviewFailure::Size)?;
    let pixels = cells
        .checked_mul(12 * 10)
        .ok_or(TeletextPreviewFailure::Size)?;
    limit(pixels, budget.max_pixels, "pixels")?;
    // Count both inventory scans, then conservatively permit an entire record
    // lookup per cell plus its 120 pixel visits. Actual candidate/origin access
    // is indexed; DRCS and authored Base replacements use binary searches.
    let work = records
        .checked_add(120)
        .and_then(|per_cell| per_cell.checked_mul(cells))
        .and_then(|render| render.checked_add(records))
        .and_then(|render| render.checked_add(pages))
        .and_then(|render| render.checked_add(base_pages))
        .ok_or(TeletextPreviewFailure::Size)?;
    limit(work, budget.max_work, "work")
}
