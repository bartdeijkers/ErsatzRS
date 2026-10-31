//! Separate read-only current guide; historical editable designs stay immutable.

use ersatzrs_domain::{
    TeletextNavigationLink, TeletextOutputGenerationObservation, TeletextPageAddress,
};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextCurrentGuidePreviewData {
    pub channel_id: i64,
    pub generation: TeletextOutputGenerationObservation,
    pub owner: TeletextPageAddress,
    pub image_url: String,
    pub pages: Vec<TeletextPageAddress>,
    pub navigation: Vec<TeletextNavigationLink>,
    pub diagnostics: Vec<super::TeletextDraftDiagnosticData>,
}

#[server(prefix = "/admin-api")]
pub async fn load_current_teletext_guide_preview(
    channel_id: i64,
    owner: TeletextPageAddress,
) -> Result<TeletextCurrentGuidePreviewData, ServerFnError> {
    let state = expect_context::<crate::host::state::AppState>();
    state
        .preview_current_guide(channel_id, owner, move |preview| {
            Ok(TeletextCurrentGuidePreviewData {
                channel_id,
                generation: preview.generation.clone(),
                owner: preview.raster.owner(),
                image_url: super::raster_image(&preview.raster)?,
                pages: preview.pages.clone(),
                navigation: preview.navigation.clone(),
                diagnostics: super::raster_diagnostics(Some(&preview.raster)),
            })
        })
        .await
        .map_err(|error| ServerFnError::new(error.code()))
}
