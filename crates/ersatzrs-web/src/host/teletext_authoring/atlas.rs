//! Read the selected base edition and render a bounded working candidate without saving it.
use super::{
    before, check_deadline, checked_selection, failure, guide, observed_selection,
    parse_native_teletext_authoring_input, prepare_static_candidate, project_teletext_preview,
    selection_digest, teletext_work, ApplicationServices, Instant, Operation,
    TeletextAuthoringError, TeletextAuthoringPrepare, TeletextAuthoringStatus,
    TeletextColourPixelOptions, TeletextFlashPhase, TeletextPageAddress, Utc,
};
use crate::admin::server_fns::teletext_atlas::{AtlasEdition, AtlasPicture};
use crate::admin::server_fns::teletext_drafts::raster_image;
use ersatzrs_app::teletext_design_documents::decode_teletext_design_documents;

pub(crate) async fn edition(
    app: &ApplicationServices,
    channel: i64,
) -> Result<AtlasEdition, TeletextAuthoringError> {
    let deadline = Instant::now() + ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT;
    let state = before(deadline, app.teletext_atlas_state(channel))
        .await?
        .map_err(|_| failure(TeletextAuthoringStatus::Unavailable))?;
    let revision = state.as_ref().map_or(0, |s| s.revision);
    let prior = state
        .as_ref()
        .and_then(|s| s.publication_key.clone().zip(s.generation));
    let guide_on = state.as_ref().and_then(|s| s.staged_guide);
    let working = state
        .filter(|s| !s.guide_pages.is_empty())
        .map(|s| serde_json::from_str(&s.guide_pages))
        .transpose()
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    let pending = before(deadline, app.teletext_publication_request(channel))
        .await?
        .map_err(|_| failure(TeletextAuthoringStatus::Unavailable))?
        .map(|p| p.id);
    let accepted = before(deadline, app.accepted_teletext_generation(channel))
        .await?
        .map_err(|_| failure(TeletextAuthoringStatus::Unavailable))?;
    let Some(generation) = accepted else {
        return Ok(AtlasEdition {
            guide_on,
            revision,
            working,
            pending,
            ..Default::default()
        });
    };
    let design = Some((generation.request.design_id, generation.request.revision));
    let operation = Operation::begin(app, deadline).await?;
    let budget = operation.policy.design;
    let stored = before(
        operation.deadline,
        app.teletext_design_revision(
            generation.request.design_id,
            generation.request.revision,
            budget.max_serialized_bytes,
        ),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::Unavailable))?
    .ok_or_else(|| failure(TeletextAuthoringStatus::MissingDesign))?;
    let maximum = operation.policy.maximum_producers();
    let (selection, layout) =
        teletext_work::spawn_blocking(operation.guard.hold_work(), move || {
            if stored.digest != generation.request.digest {
                return Err(failure(TeletextAuthoringStatus::InvalidInput));
            }
            if stored.document_version
                == ersatzrs_domain::TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION
            {
                // The scripted scope keeps its own editor. The base form still
                // fences publication against the accepted generation.
                return Ok((None, None));
            }
            let parts = decode_teletext_design_documents(&stored.document, budget)
                .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?
                .into_parts();
            if parts.selection.channel_id != channel {
                return Err(failure(TeletextAuthoringStatus::WrongChannel));
            }
            let selection = selection_digest(&parts.selection, maximum, deadline)?;
            let layout = parts
                .guide_inputs
                .and_then(|inputs| inputs.page_layout())
                .map(|raw| {
                    // Library destinations are request metadata. Their editable instances
                    // come from the channel element links, including after a restart.
                    let mut pages: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
                    for page in &mut pages {
                        if let Some(fields) = page.as_object_mut() {
                            fields.remove("index_entries");
                        }
                    }
                    serde_json::from_value(pages.into())
                })
                .transpose()
                .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
            Ok((Some(selection), layout))
        })
        .await
        .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))??;
    let publication_key = prior.filter(|(_, id)| *id == generation.id).map(|p| p.0);
    Ok(AtlasEdition {
        publication_key,
        design,
        guide_on,
        accepted: Some(generation.id),
        pending,
        revision,
        working,
        selection,
        layout,
    })
}

pub(crate) async fn picture(
    state: &crate::host::state::AppState,
    request: TeletextAuthoringPrepare,
    elements: &[ersatzrs_domain::TeletextAtlasElement],
    owner: TeletextPageAddress,
    reveal: bool,
    deadline: Instant,
) -> Result<AtlasPicture, TeletextAuthoringError> {
    let app = &state.application;
    let operation = Operation::begin(app, deadline).await?;
    let input = parse_native_teletext_authoring_input(&request.input_json, &operation.policy)
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    let selected = observed_selection(app, request.channel_id, operation.deadline).await?;
    // An unbound channel has an explicit Off observation and no selection digest.
    // Accept that observation only here, for a read-only picture of the same empty set.
    let selected = if selected.bindings.is_empty() && request.selection_digest.is_empty() {
        selected
    } else {
        checked_selection(selected, &request.selection_digest, &operation).await?
    };
    if input.guide_channel_ids().is_some()
        && !selected
            .bindings
            .iter()
            .any(|s| s.addon_id == input.entry_addon_id())
    {
        return unbound_picture(state, &input, &operation, elements, owner, reveal).await;
    }
    let candidate = if input.guide_channel_ids().is_some() {
        guide::preview_base(app, &state.pool, &input, &selected, &operation, elements).await?
    } else {
        prepare_static_candidate(app, &input, &selected, &operation).await?
    };
    let budget = operation.policy.preview;
    teletext_work::spawn_blocking(operation.guard.hold_work(), move || {
        let service = candidate
            .encoded()
            .ok_or_else(|| failure(TeletextAuthoringStatus::Off))?;
        render_picture(service, owner, reveal, budget, deadline)
    })
    .await
    .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?
}

async fn unbound_picture(
    state: &crate::host::state::AppState,
    input: &crate::host::teletext_runtime_policy::NativeTeletextAuthoringInput,
    operation: &Operation,
    elements: &[ersatzrs_domain::TeletextAtlasElement],
    owner: TeletextPageAddress,
    reveal: bool,
) -> Result<AtlasPicture, TeletextAuthoringError> {
    let deadline = operation.deadline;
    let (bytes, generation) = guide::preview_unbound_guide(&state.pool, input, operation).await?;
    let policy = operation
        .policy
        .encoding_policy(input)
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    if bytes.len() > policy.input.max_response_bytes {
        return Err(failure(TeletextAuthoringStatus::ResourceLimit));
    }
    let plan = before(
        deadline,
        state.application.preview_teletext_instances(
            input.channel_id(),
            elements,
            input.page_layout.as_deref(),
            Utc::now(),
        ),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::CustomPagesBlocked))?;
    if !plan.is_ready() {
        return Err(failure(TeletextAuthoringStatus::CustomPagesBlocked));
    }
    let budget = operation.policy.preview;
    teletext_work::spawn_blocking(operation.guard.hold_work(), move || {
        use ersatzrs_app::teletext_profile_documents::TeletextProfileDocument;
        let (guide, _) = ersatzrs_app::teletext_pages_v3::parse_authored_service_with_metadata(
            &bytes,
            &generation,
            policy.validation.service,
            policy.validation.declarations,
        )
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
        let mut documents = vec![TeletextProfileDocument::Authored(Box::new(guide))];
        if !plan.pages.is_empty() {
            let custom = ersatzrs_domain::TeletextCustomPagesDocument::from_wire(
                ersatzrs_domain::TeletextCustomPagesWire {
                    schema: ersatzrs_domain::TELETEXT_CUSTOM_PAGES_SCHEMA.into(),
                    generation: generation.clone(),
                    pages: plan.pages,
                },
                &generation,
            )
            .map_err(|_| failure(TeletextAuthoringStatus::CustomPagesBlocked))?;
            documents.push(TeletextProfileDocument::CustomPages(Box::new(custom)));
        }
        let encoded = crate::host::teletext_channel_documents::encode_original_producer_documents(
            Some(0),
            documents,
            vec![],
            deadline,
            None,
            policy,
        )
        .map_err(|_| failure(TeletextAuthoringStatus::PreviewFailed))?;
        render_picture(
            encoded
                .encoded
                .as_ref()
                .ok_or_else(|| failure(TeletextAuthoringStatus::Off))?,
            owner,
            reveal,
            budget,
            deadline,
        )
    })
    .await
    .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?
}

fn render_picture(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    reveal: bool,
    budget: crate::host::teletext_preview::TeletextPreviewBudget,
    deadline: Instant,
) -> Result<AtlasPicture, TeletextAuthoringError> {
    let mut preview = project_teletext_preview(
        service,
        owner,
        TeletextColourPixelOptions {
            reveal_concealed: reveal,
            flash_phase: TeletextFlashPhase::Original,
        },
        budget,
    )
    .map_err(|_| failure(TeletextAuthoringStatus::PreviewFailed))?;
    let hits = std::mem::take(&mut preview.hits);
    let (raster, navigation) = preview.into_parts();
    check_deadline(deadline)?;
    Ok(AtlasPicture {
        hits,
        image: raster_image(&raster)?,
        owner,
        pages: service
            .declared()
            .semantic()
            .base()
            .pages
            .iter()
            .map(|p| p.address)
            .collect(),
        navigation: navigation.to_vec(),
    })
}

pub(crate) fn fingerprint(bytes: &[u8]) -> String {
    crate::host::teletext_guide_refresh_native::digest(bytes)
}

pub(crate) async fn element_picture(
    state: &crate::host::state::AppState,
    channel: i64,
    elements: &[ersatzrs_domain::TeletextAtlasElement],
    guide_layout: Option<&str>,
    owner: TeletextPageAddress,
    reveal: bool,
    deadline: Instant,
) -> Result<AtlasPicture, TeletextAuthoringError> {
    use ersatzrs_app::teletext_profile_documents::TeletextProfileDocument;
    use ersatzrs_domain::{
        TeletextCustomPagesDocument, TeletextCustomPagesWire, TELETEXT_CUSTOM_PAGES_SCHEMA,
    };
    let operation = Operation::begin(&state.application, deadline).await?;
    let plan = before(
        operation.deadline,
        state
            .application
            .preview_teletext_instances(channel, elements, guide_layout, Utc::now()),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    if !plan.is_ready() {
        return Err(failure(TeletextAuthoringStatus::CustomPagesBlocked));
    }
    let preview = operation.policy.preview;
    let mut policy = operation.policy.reload_encoding_policy();
    teletext_work::spawn_blocking(operation.guard.hold_work(), move || {
        let mut pages = plan.pages;
        let mut links = Vec::new();
        // Cross-producer links are resolved against the complete channel on save.
        // This picture needs only the selected element's rows and declared shortcuts.
        for page in &mut pages {
            if page.page == owner.page.get() && page.subcode == owner.subcode.get() {
                links.clone_from(&page.channel_links);
            }
            page.channel_links.clear();
        }
        let document = TeletextCustomPagesDocument::from_wire(
            TeletextCustomPagesWire {
                schema: TELETEXT_CUSTOM_PAGES_SCHEMA.into(),
                generation: "preview".into(),
                pages,
            },
            "preview",
        )
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
        policy.entry = document
            .draft()
            .pages
            .iter()
            .find(|p| p.address.page == document.draft().initial_page)
            .map(|p| p.address);
        let region = policy.v1_region;
        let service = crate::host::teletext_channel_documents::encode_original_producer_documents(
            Some(0),
            vec![TeletextProfileDocument::CustomPages(Box::new(document))],
            vec![],
            deadline,
            Some(region),
            policy,
        )
        .map_err(|_| failure(TeletextAuthoringStatus::PreviewFailed))?
        .encoded
        .ok_or_else(|| failure(TeletextAuthoringStatus::Off))?;
        let mut projection = project_teletext_preview(
            &service,
            owner,
            TeletextColourPixelOptions {
                reveal_concealed: reveal,
                flash_phase: TeletextFlashPhase::Original,
            },
            preview,
        )
        .map_err(|_| failure(TeletextAuthoringStatus::PreviewFailed))?;
        let hits = std::mem::take(&mut projection.hits);
        let (raster, _) = projection.into_parts();
        Ok(AtlasPicture {
            hits,
            image: raster_image(&raster)?,
            owner,
            pages: service
                .declared()
                .semantic()
                .base()
                .pages
                .iter()
                .map(|p| p.address)
                .collect(),
            navigation: links,
        })
    })
    .await
    .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?
}
