//! Immutable native preview projection for SSR and the agent file workflow.

use ersatzrs_domain::{TeletextNavigationLink, TeletextPageAddress};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Read-only choices from the complete admitted native service; never edit authority.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextObjectChoicesObservation {
    pub pages: Vec<TeletextObjectPageChoiceObservation>,
    pub calls: Vec<TeletextObjectCallChoiceObservation>,
    pub definitions: Vec<TeletextObjectDefinitionChoiceObservation>,
}

/// Selected POP/GPOP roles; defaults and original resolution provenance are unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextObjectPageChoiceObservation {
    pub display: TeletextPageAddress,
    /// Only POP/GPOP occur in native observations; submitted kinds are untrusted.
    pub role: ersatzrs_domain::TeletextResourceKind,
    pub target: ersatzrs_domain::TeletextRawDataPageNumber,
    pub required_subpages: ersatzrs_domain::TeletextSubpageRequirementMask,
}

/// Actual resource-body calls, with their original ordinal, type and offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextObjectCallChoiceObservation {
    pub caller: ersatzrs_domain::TeletextObjectIdentity,
    pub command_index: usize,
    pub target: ersatzrs_domain::TeletextObjectIdentity,
    pub object_type: ersatzrs_domain::TeletextObjectType,
    pub offset: Option<ersatzrs_domain::TeletextObjectOffset>,
}

/// Existing definitions retain identity and exact declared type, without permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextObjectDefinitionChoiceObservation {
    pub identity: ersatzrs_domain::TeletextObjectIdentity,
    pub object_type: ersatzrs_domain::TeletextObjectType,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextDraftPreviewData {
    pub channel_id: i64,
    pub design_id: i64,
    pub revision: i64,
    pub owner: Option<TeletextPageAddress>,
    pub image_url: Option<String>,
    pub effective: bool,
    pub valid_until: Option<String>,
    pub pages: Vec<TeletextPageAddress>,
    pub resource_objects: Vec<ersatzrs_domain::TeletextObjectIdentity>,
    pub object_resources: Vec<ersatzrs_domain::TeletextResourceIdentity>,
    pub object_choices: TeletextObjectChoicesObservation,
    pub navigation: Vec<TeletextNavigationLink>,
    pub diagnostics: Vec<TeletextDraftDiagnosticData>,
    pub authoring: Vec<TeletextDraftAuthoringData>,
    /// Unavailable observation remains distinct from observed absent pointers.
    pub publication: Option<TeletextDraftPublicationObservation>,
    /// Ephemeral per-output diagnostics, separate from durable selection.
    #[serde(default)]
    pub outputs: Option<ersatzrs_domain::TeletextChannelOutputObservations>,
}

/// Historical declarations for the selected owner, never submitted permissions.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextDraftAuthoringData {
    pub producer_index: usize,
    pub binding_index: usize,
    pub addon_id: String,
    pub template: ersatzrs_addon_contract::TeletextAuthoringTemplateWire,
    pub theme: ersatzrs_addon_contract::TeletextAuthoringThemeWire,
    pub regions: Vec<ersatzrs_addon_contract::TeletextAuthoringRegionWire>,
    pub dynamic_fields: Vec<ersatzrs_addon_contract::TeletextAuthoringDynamicFieldWire>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextDraftPublicationState {
    Unrequested,
    Pending,
    Selected,
    Unavailable,
}

/// Read-only rendered CAS observations; mutation requires fresh native checks.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextDraftPublicationObservation {
    pub digest: String,
    pub current_design_head: i64,
    pub current_pending_id: Option<i64>,
    pub current_accepted_id: Option<i64>,
    pub state: TeletextDraftPublicationState,
    pub can_request: bool,
    pub can_publish: bool,
    pub replaces_pending: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextDraftDiagnosticData {
    pub owner: TeletextPageAddress,
    pub visual_row: u8,
    pub visual_column: u8,
    pub original_position: Option<(u16, u16)>,
    pub category: String,
    pub candidate: Option<usize>,
    pub source: Option<String>,
    pub invocation: Option<usize>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextDraftErrorLocation {
    pub owner: Option<TeletextPageAddress>,
    pub resource: Option<ersatzrs_domain::TeletextResourceIdentity>,
    pub position: Option<(u16, u16)>,
    pub source: Option<String>,
}

/// Static outcomes, independent of original source and configuration contents.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextDraftPreviewResult {
    pub preview: Option<TeletextDraftPreviewData>,
    pub error: Option<String>,
    pub error_location: Option<TeletextDraftErrorLocation>,
}

#[server(prefix = "/admin-api")]
pub async fn load_teletext_draft_preview(
    channel_id: i64,
    design_id: i64,
    revision: i64,
    owner: Option<TeletextPageAddress>,
) -> Result<TeletextDraftPreviewResult, ServerFnError> {
    let state = expect_context::<crate::host::state::AppState>();
    let result = load_preview(&state.application, channel_id, design_id, revision, owner).await;
    Ok(match result {
        Ok(preview) => TeletextDraftPreviewResult {
            preview: Some(preview),
            error: None,
            error_location: None,
        },
        Err(error) => TeletextDraftPreviewResult {
            preview: None,
            error: Some(error.code().to_owned()),
            error_location: error.location.map(|location| TeletextDraftErrorLocation {
                owner: location.owner,
                resource: location.resource,
                position: location.position,
                source: location.source.map(|source| format!("{source:?}")),
            }),
        },
    })
}

#[cfg(feature = "ssr")]
async fn load_preview(
    application: &ersatzrs_app::ApplicationServices,
    channel_id: i64,
    design_id: i64,
    revision: i64,
    owner: Option<TeletextPageAddress>,
) -> Result<TeletextDraftPreviewData, crate::host::teletext_authoring::TeletextAuthoringError> {
    use crate::host::teletext_authoring::{
        preview_original_teletext_design, TeletextAuthoringError, TeletextAuthoringPreviewRequest,
        TeletextAuthoringStatus,
    };
    let invalid = || TeletextAuthoringError {
        status: TeletextAuthoringStatus::InvalidInput,
        location: None,
    };
    let deadline = std::time::Instant::now()
        .checked_add(ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT)
        .ok_or_else(invalid)?;
    let preview = preview_original_teletext_design(
        application,
        TeletextAuthoringPreviewRequest {
            channel_id,
            design_id,
            revision,
            owner,
        },
        deadline,
    )
    .await?;
    let mut projected = preview.project(project_preview).await?;
    projected.outputs = application.teletext_output_observations(channel_id);
    Ok(projected)
}

#[cfg(feature = "ssr")]
fn project_preview(
    preview: &crate::host::teletext_authoring::TeletextAuthoringPreview,
) -> Result<TeletextDraftPreviewData, crate::host::teletext_authoring::TeletextAuthoringError> {
    use ersatzrs_ffmpeg::teletext::TeletextRasterCellFailure as Failure;
    let raster = preview.raster();
    let diagnostics = raster
        .into_iter()
        .flat_map(ersatzrs_ffmpeg::teletext::TeletextRaster::diagnostics)
        .map(|diagnostic| {
            let category = match &diagnostic.reason {
                Failure::MissingDisplayCell | Failure::Display { .. } => "display",
                Failure::GlyphIntent { .. } | Failure::MissingGlyph => "glyph",
                Failure::MaskPosition | Failure::Mask(_) => "mask",
                Failure::GlyphAttribute { .. } => "attribute",
                Failure::Colour(_) => "colour",
            };
            TeletextDraftDiagnosticData {
                owner: diagnostic.owner,
                visual_row: diagnostic.visual_position.0,
                visual_column: diagnostic.visual_position.1,
                original_position: diagnostic.original_position,
                category: category.to_owned(),
                candidate: diagnostic.candidate,
                // These checked engine enums contain numeric provenance only.
                source: diagnostic.location.map(|source| format!("{source:?}")),
                invocation: diagnostic
                    .invocation
                    .map(ersatzrs_domain::TeletextSpatialInvocationId::get),
            }
        })
        .collect();
    Ok(TeletextDraftPreviewData {
        channel_id: preview.identity.channel_id,
        design_id: preview.identity.design_id,
        revision: preview.identity.revision,
        owner: raster.map(ersatzrs_ffmpeg::teletext::TeletextRaster::owner),
        image_url: raster.map(raster_image).transpose()?,
        effective: preview.scope()
            == crate::host::teletext_authoring::TeletextAuthoringScope::Effective,
        valid_until: preview.valid_until().map(|until| until.to_rfc3339()),
        pages: preview.pages().to_vec(),
        resource_objects: preview.resource_objects().to_vec(),
        object_resources: preview.object_resources().to_vec(),
        object_choices: preview.object_choices().clone(),
        navigation: preview.navigation().to_vec(),
        diagnostics,
        authoring: preview.authoring().iter().map(project_authoring).collect(),
        publication: preview.publication().map(project_publication),
        outputs: None,
    })
}

#[cfg(feature = "ssr")]
fn project_authoring(
    metadata: &crate::host::teletext_authoring::TeletextAuthoringPageMetadata,
) -> TeletextDraftAuthoringData {
    TeletextDraftAuthoringData {
        producer_index: metadata.producer_index,
        binding_index: metadata.binding_index,
        addon_id: metadata.addon_id.clone(),
        template: metadata.template.clone(),
        theme: metadata.theme.clone(),
        regions: metadata.regions.clone(),
        dynamic_fields: metadata.dynamic_fields.clone(),
    }
}

#[cfg(feature = "ssr")]
fn project_publication(
    observation: &crate::host::teletext_authoring::TeletextAuthoringPublicationObservation,
) -> TeletextDraftPublicationObservation {
    use crate::host::teletext_authoring::TeletextAuthoringPublicationState as State;
    let state = match observation.state {
        State::Unrequested => TeletextDraftPublicationState::Unrequested,
        State::Pending => TeletextDraftPublicationState::Pending,
        State::Selected => TeletextDraftPublicationState::Selected,
        State::Unavailable => TeletextDraftPublicationState::Unavailable,
    };
    TeletextDraftPublicationObservation {
        digest: observation.digest.clone(),
        current_design_head: observation.current_design_head,
        current_pending_id: observation.current_pending_id,
        current_accepted_id: observation.current_accepted_id,
        state,
        can_request: observation.can_request,
        can_publish: observation.can_publish,
        replaces_pending: observation.replaces_pending,
    }
}

#[cfg(feature = "ssr")]
fn raster_image(
    raster: &ersatzrs_ffmpeg::teletext::TeletextRaster,
) -> Result<String, crate::host::teletext_authoring::TeletextAuthoringError> {
    use base64::Engine as _;
    use std::fmt::Write as _;
    let error = || crate::host::teletext_authoring::TeletextAuthoringError {
        status: crate::host::teletext_authoring::TeletextAuthoringStatus::PreviewFailed,
        location: None,
    };
    // Typed pixels are the only source of generated SVG. Each run uses fewer
    // than 96 bytes; checked storage bounds precede serialization/allocation.
    let maximum = raster
        .pixels()
        .len()
        .checked_mul(96)
        .and_then(|bytes| bytes.checked_add(2048))
        .ok_or_else(error)?;
    let mut svg = String::new();
    svg.try_reserve_exact(maximum).map_err(|_| error())?;
    write!(svg, "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" shape-rendering=\"crispEdges\">", raster.width(), raster.height()).map_err(|_| error())?;
    svg.push_str("<defs><pattern id=\"header\" width=\"8\" height=\"8\" patternUnits=\"userSpaceOnUse\"><rect width=\"8\" height=\"8\" fill=\"#aaa\"/><path d=\"M0 0L8 8M8 0L0 8\" stroke=\"#777\"/></pattern><pattern id=\"video\" width=\"8\" height=\"8\" patternUnits=\"userSpaceOnUse\"><rect width=\"8\" height=\"8\" fill=\"#222\"/><rect width=\"4\" height=\"4\" fill=\"#888\"/></pattern><pattern id=\"unresolved\" width=\"8\" height=\"8\" patternUnits=\"userSpaceOnUse\"><rect width=\"8\" height=\"8\" fill=\"#f0f\"/><path d=\"M0 0L8 8\" stroke=\"#000\"/></pattern></defs>");
    for (y, row) in raster.pixels().chunks_exact(raster.width()).enumerate() {
        let mut start = 0;
        while start < row.len() {
            let pixel = preview_pixel(row[start]);
            let fill = pixel_fill(pixel);
            let mut end = start + 1;
            while end < row.len() && preview_pixel(row[end]) == pixel {
                end += 1;
            }
            write!(
                svg,
                "<rect x=\"{start}\" y=\"{y}\" width=\"{}\" height=\"1\" fill=\"{fill}\"/>",
                end - start
            )
            .map_err(|_| error())?;
            start = end;
        }
    }
    svg.push_str("</svg>");
    if svg.len() > maximum {
        return Err(error());
    }
    let encoded_maximum = svg
        .len()
        .checked_mul(4)
        .and_then(|bytes| bytes.checked_add(64))
        .ok_or_else(error)?;
    let mut url = String::from("data:image/svg+xml;base64,");
    url.try_reserve_exact(encoded_maximum)
        .map_err(|_| error())?;
    base64::engine::general_purpose::STANDARD.encode_string(svg.as_bytes(), &mut url);
    Ok(url)
}

#[cfg(feature = "ssr")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum PreviewPixel {
    Rgb([u8; 3]),
    Video,
    Header,
    Unresolved,
}

#[cfg(feature = "ssr")]
fn preview_pixel(pixel: ersatzrs_ffmpeg::teletext::TeletextRasterPixel) -> PreviewPixel {
    use ersatzrs_domain::TeletextColourPixel;
    use ersatzrs_ffmpeg::teletext::TeletextRasterPixel;
    match pixel {
        TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) => PreviewPixel::Rgb(rgb.get()),
        TeletextRasterPixel::Colour(TeletextColourPixel::Video) => PreviewPixel::Video,
        TeletextRasterPixel::ReceiverHeader => PreviewPixel::Header,
        TeletextRasterPixel::Unresolved(_) => PreviewPixel::Unresolved,
    }
}

#[cfg(feature = "ssr")]
fn pixel_fill(pixel: PreviewPixel) -> String {
    match pixel {
        PreviewPixel::Rgb([r, g, b]) => {
            format!("#{r:x}{r:x}{g:x}{g:x}{b:x}{b:x}")
        }
        PreviewPixel::Video => "url(#video)".to_owned(),
        PreviewPixel::Header => "url(#header)".to_owned(),
        PreviewPixel::Unresolved => "url(#unresolved)".to_owned(),
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::host::teletext_authoring::{
        TeletextAuthoringPublicationObservation, TeletextAuthoringPublicationState as State,
    };

    #[test]
    fn authoring_projection_preserves_typed_regions_bindings_and_public_overrides() {
        use crate::host::teletext_authoring::TeletextAuthoringPageMetadata;
        let captured: ersatzrs_addon_contract::TeletextAuthoringMetadataWire =
            serde_json::from_value(serde_json::json!({
                "schema_version":1,"template":{"id":"synthetic-grid","revision":"r7"},
                "theme":{"id":"night","overrides":[
                    {"key":"accent","value":{"kind":"colour","red":0,"green":1,"blue":15}},
                    {"key":"layout","value":{"kind":"layout","id":"compact"}},
                    {"key":"spacing","value":{"kind":"number","value":-2}},
                    {"key":"footer","value":{"kind":"boolean","value":false}}
                ]},
                "pages":[{"page":{"page":124,"subcode":32},"regions":[
                    {"id":"title","kind":"free","area":"main","row":2,"column":1,"width":20,"height":1},
                    {"id":"custom","kind":"zone","area":"left_panel","row":3,"column":0,"width":4,"height":2},
                    {"id":"guide","kind":"generated","area":"right_panel","row":4,"column":1,"width":3,"height":1}
                ],"dynamic_fields":[{"id":"guide-title","region":"guide","binding":"guide.title"}]}]
            })).unwrap();
        let page = &captured.pages[0];
        let metadata = TeletextAuthoringPageMetadata {
            producer_index: 1,
            binding_index: 3,
            addon_id: "synthetic.authoring".to_owned(),
            template: captured.template.clone(),
            theme: captured.theme.clone(),
            regions: page.regions.clone(),
            dynamic_fields: page.dynamic_fields.clone(),
        };
        let projected = project_authoring(&metadata);
        let json = serde_json::to_value(&projected).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 7);
        assert_eq!(json["producer_index"], 1);
        assert_eq!(json["binding_index"], 3);
        assert_eq!(json["addon_id"], "synthetic.authoring");
        assert_eq!(
            json["template"],
            serde_json::to_value(&captured.template).unwrap()
        );
        assert_eq!(
            json["theme"],
            serde_json::to_value(&captured.theme).unwrap()
        );
        assert_eq!(
            json["regions"],
            serde_json::to_value(&page.regions).unwrap()
        );
        assert_eq!(
            json["dynamic_fields"],
            serde_json::to_value(&page.dynamic_fields).unwrap()
        );
        let roundtrip: TeletextDraftAuthoringData = serde_json::from_value(json).unwrap();
        assert!(roundtrip == projected);
        let debug = format!("{metadata:?}");
        for text in [
            "synthetic.authoring",
            "synthetic-grid",
            "night",
            "guide.title",
            "compact",
        ] {
            assert!(
                !debug.contains(text),
                "preview metadata Debug must remain redacted"
            );
        }
    }

    #[test]
    fn publication_projection_preserves_observed_fences_and_unavailable_observation() {
        let absent: Option<TeletextAuthoringPublicationObservation> = None;
        assert_eq!(
            serde_json::to_value(absent.as_ref().map(project_publication)).unwrap(),
            serde_json::Value::Null
        );
        for (state, serialized_state) in [
            (State::Unrequested, "unrequested"),
            (State::Pending, "pending"),
            (State::Selected, "selected"),
            (State::Unavailable, "unavailable"),
        ] {
            for pointers in [(None, None), (Some(19), Some(23))] {
                let observation = TeletextAuthoringPublicationObservation {
                    digest: "a".repeat(64),
                    current_design_head: 7,
                    current_pending_id: pointers.0,
                    current_accepted_id: pointers.1,
                    state,
                    can_request: state == State::Unrequested,
                    can_publish: state == State::Pending,
                    replaces_pending: state == State::Selected,
                };
                let projected = project_publication(&observation);
                assert_eq!(projected.digest, observation.digest);
                assert_eq!(projected.current_design_head, 7);
                assert_eq!(projected.current_pending_id, pointers.0);
                assert_eq!(projected.current_accepted_id, pointers.1);
                assert_eq!(projected.can_request, observation.can_request);
                assert_eq!(projected.can_publish, observation.can_publish);
                assert_eq!(projected.replaces_pending, observation.replaces_pending);
                let value = serde_json::to_value(&projected).unwrap();
                assert_eq!(value["state"], serialized_state);
                let restored: TeletextDraftPublicationObservation =
                    serde_json::from_value(value).unwrap();
                assert!(restored == projected);
            }
        }
    }
}
