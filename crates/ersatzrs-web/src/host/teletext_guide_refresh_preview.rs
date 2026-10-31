//! Read-only current guide raster from the ordinary output's sealed service.

use std::time::Instant;

use ersatzrs_app::ApplicationServices;
use ersatzrs_domain::{
    TeletextColourPixelOptions, TeletextFlashPhase, TeletextNavigationLink,
    TeletextOutputGenerationObservation, TeletextPageAddress,
};
use ersatzrs_ffmpeg::teletext::TeletextRaster;
use ersatzrs_infra::db::DbPool;

use super::{
    teletext_authoring::{TeletextAuthoringError, TeletextAuthoringStatus},
    teletext_preview::project_teletext_preview,
    teletext_runtime_policy::{
        parse_native_teletext_authoring_policy, parse_native_teletext_runtime_policy,
        NATIVE_TELETEXT_AUTHORING_POLICY_SETTING, NATIVE_TELETEXT_RUNTIME_POLICY_SETTING,
    },
    teletext_work,
};

pub(crate) struct CurrentGuidePreview {
    pub(crate) generation: TeletextOutputGenerationObservation,
    pub(crate) raster: TeletextRaster,
    pub(crate) pages: Vec<TeletextPageAddress>,
    pub(crate) navigation: Vec<TeletextNavigationLink>,
}

impl super::state::AppState {
    pub(crate) async fn preview_current_guide<T: Send + 'static>(
        &self,
        channel_id: i64,
        owner: TeletextPageAddress,
        projection: impl FnOnce(&CurrentGuidePreview) -> Result<T, TeletextAuthoringError>
            + Send
            + 'static,
    ) -> Result<T, TeletextAuthoringError> {
        preview_current_guide(&self.application, &self.pool, channel_id, owner, projection).await
    }
}

/// Never invokes a producer or changes manual concepts/publication pointers.
/// Recovery, rendering and final current-state confirmation share one deadline;
/// actual rendering workers retain their admission after requester cancellation.
async fn preview_current_guide<T: Send + 'static>(
    application: &ApplicationServices,
    pool: &DbPool,
    channel_id: i64,
    owner: TeletextPageAddress,
    projection: impl FnOnce(&CurrentGuidePreview) -> Result<T, TeletextAuthoringError> + Send + 'static,
) -> Result<T, TeletextAuthoringError> {
    let started = Instant::now();
    let deadline = started
        .checked_add(ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT)
        .ok_or_else(unavailable)?;
    let work = async {
        if channel_id <= 0 {
            return Err(unavailable());
        }
        let (runtime, authoring, deadline) = configuration(application, started, deadline).await?;
        let guide = crate::transport::iptv::GuideRuntime {
            pool: pool.clone(),
            limits: runtime.guide_limits.ok_or_else(unavailable)?,
            refresh: true,
        };
        let maximum = runtime.design.max_serialized_bytes;
        let cadence = runtime
            .live_updates
            .ok_or_else(unavailable)?
            .poll_interval();
        let recovery = crate::transport::iptv::recover_current_guide_for_preview(
            application,
            channel_id,
            runtime.design,
            runtime.encoding,
            deadline,
            &guide,
            cadence,
        );
        let candidate = tokio::time::timeout_at(deadline.into(), recovery)
            .await
            .map_err(|_| failed_at("recovery"))?
            .map_err(|_| failed_at("recovery"))?
            .ok_or_else(unavailable)?;
        let deadline = candidate
            .next_transition()
            .map_or(deadline, |cap| deadline.min(cap.into_std()));
        let generation = candidate.output_observation().ok_or_else(unavailable)?;
        if generation.guide_refresh.is_none() {
            return Err(unavailable());
        }
        let service = candidate.service().cloned().ok_or_else(unavailable)?;
        let guard = crate::transport::iptv::begin_current_guide_preview_work(
            application,
            deadline,
            cadence,
        )
        .await
        .map_err(|error| {
            tracing::debug!(%error, "current teletext guide preview admission failed");
            failed_at("render_admission")
        })?;
        let worker = teletext_work::spawn_blocking(guard.hold_work(), move || {
            render_current_guide(
                &service,
                owner,
                generation,
                authoring.preview,
                projection,
                deadline,
            )
        });
        let rendered = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), worker)
            .await
            .map_err(|_| failed_at("render_timeout"))?
            .map_err(|_| failed_at("render_worker"))??;
        if !candidate
            .confirm_guide_preview_with_hold(application, maximum, deadline, guard.hold_work())
            .await
        {
            return Err(failed_at("final_confirmation"));
        }
        drop(guard);
        Ok(rendered)
    };
    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), Box::pin(work))
        .await
        .map_err(|_| unavailable())?
}

fn render_current_guide<T>(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    generation: TeletextOutputGenerationObservation,
    budget: super::teletext_preview::TeletextPreviewBudget,
    projection: impl FnOnce(&CurrentGuidePreview) -> Result<T, TeletextAuthoringError>,
    deadline: Instant,
) -> Result<T, TeletextAuthoringError> {
    if Instant::now() >= deadline {
        return Err(unavailable());
    }
    let preview = project_teletext_preview(
        service,
        owner,
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        budget,
    )
    .map_err(|_| failed_at("raster_projection"))?;
    let pages = service
        .declared()
        .semantic()
        .base()
        .pages
        .iter()
        .map(|p| p.address)
        .collect();
    let (raster, navigation) = preview.into_parts();
    let result = projection(&CurrentGuidePreview {
        generation,
        raster,
        pages,
        navigation: navigation.to_vec(),
    })
    .map_err(|_| failed_at("projection"))?;
    if Instant::now() >= deadline {
        return Err(unavailable());
    }
    Ok(result)
}

async fn configuration(
    application: &ApplicationServices,
    started: Instant,
    deadline: Instant,
) -> Result<
    (
        super::teletext_runtime_policy::NativeTeletextRuntimePolicy,
        super::teletext_runtime_policy::NativeTeletextAuthoringPolicy,
        Instant,
    ),
    TeletextAuthoringError,
> {
    let raw = application
        .get_optional_setting(NATIVE_TELETEXT_RUNTIME_POLICY_SETTING)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
    let runtime = parse_native_teletext_runtime_policy(&raw).map_err(|_| unavailable())?;
    if !runtime.guide_refresh {
        return Err(unavailable());
    }
    let authoring = application
        .get_optional_setting(NATIVE_TELETEXT_AUTHORING_POLICY_SETTING)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
    let authoring =
        parse_native_teletext_authoring_policy(&authoring).map_err(|_| unavailable())?;
    let deadline = started
        .checked_add(authoring.operation_timeout)
        .ok_or_else(unavailable)?
        .min(deadline);
    Ok((runtime, authoring, deadline))
}

fn unavailable() -> TeletextAuthoringError {
    TeletextAuthoringError {
        status: TeletextAuthoringStatus::PreviewFailed,
        location: None,
    }
}

fn failed_at(stage: &'static str) -> TeletextAuthoringError {
    tracing::debug!(stage, "current teletext guide preview failed");
    unavailable()
}
