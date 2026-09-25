//! Native POST adapter: navigation carries input; only explicit commands persist.
use super::{Draft, Submission};
use crate::admin::i18n::{tr, TextKey};
use crate::host::state::AppState;
use axum::{
    body::{to_bytes, Body},
    extract::{FromRequest, Multipart, State},
    http::{header, Method, Request, StatusCode},
    response::{IntoResponse, Response},
};
use ersatzrs_core::repositories::{
    AddonMediaListArtwork, AddonMediaListArtworkRole, AddonMediaListDraftOptions,
    AddonMediaListEditorItem, AddonMediaListEditorMetadata, AddonMediaListEditorSave,
    AddonMediaListEditorSettings, AddonMediaListImportAction, AddonMediaListItemAdminRow,
    AddonMediaListItemReviewEdit, AddonMediaListNfoMetadata,
};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListDetailPolicy, AddonMediaListPresentationKind,
    AddonMediaListRecoveryAction, RemoteStreamLiveness,
};
use leptos::prelude::*;
use std::collections::BTreeMap;

mod bulk;
mod lifecycle;
use bulk::preview_bulk;

pub const MAX_FORM_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_REQUEST_BYTES: usize =
    MAX_FORM_BYTES + 3 * ersatzrs_infra::addon_artwork::MAX_ARTWORK_BYTES + 64 * 1024;
type Fields = BTreeMap<String, String>;
type Result<T> = std::result::Result<T, TextKey>;

#[expect(
    clippy::too_many_lines,
    reason = "The bounded native request adapter keeps parse, error retention and rendering in one flow"
)]
pub async fn post(State(state): State<AppState>, request: Request<Body>) -> Response {
    let (mut parts, body) = request.into_parts();
    let path_id = parts
        .uri
        .path()
        .split('/')
        .nth(3)
        .and_then(|s| s.parse::<i64>().ok());
    let multipart = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/form-data"));
    let Ok(body) = to_bytes(
        body,
        if multipart {
            MAX_REQUEST_BYTES
        } else {
            MAX_FORM_BYTES
        },
    )
    .await
    else {
        return size_error(&state);
    };
    let mut uploads = BTreeMap::new();
    let fields: Fields = if multipart {
        let request = Request::from_parts(parts.clone(), Body::from(body));
        let Ok(mut multipart) = Multipart::from_request(request, &state).await else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let mut fields = Fields::new();
        let mut field_bytes = 0usize;
        loop {
            let field = match multipart.next_field().await {
                Ok(Some(field)) => field,
                Ok(None) => break,
                Err(_) => return StatusCode::BAD_REQUEST.into_response(),
            };
            let name = field.name().unwrap_or_default().to_owned();
            if name.starts_with("upload_") {
                let content_type = field.content_type().unwrap_or_default().to_owned();
                let Ok(bytes) = field.bytes().await else {
                    return StatusCode::BAD_REQUEST.into_response();
                };
                if !bytes.is_empty() {
                    uploads.insert(name, (content_type, bytes));
                }
            } else {
                let Ok(value) = field.text().await else {
                    return StatusCode::BAD_REQUEST.into_response();
                };
                field_bytes = field_bytes.saturating_add(name.len() + value.len());
                if field_bytes > MAX_FORM_BYTES {
                    return size_error(&state);
                }
                fields.insert(name, value);
            }
        }
        fields
    } else {
        let Ok(pairs) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&body) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        pairs.into_iter().collect()
    };
    let Some(encoded) = fields.get("draft") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(mut draft) = serde_json::from_str::<Draft>(encoded) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if Some(draft.list_id) != path_id
        || draft.list_id <= 0
        || draft.groups.len() > 10_010
        || ![0, 10, 25, 50, 100].contains(&draft.page_size)
        || !super::valid_sort(&draft.sort)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let group = fields.get("group").cloned().unwrap_or_default();
    if !valid_group(&group) || draft.groups.keys().any(|g| !valid_group(g)) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    merge(&mut draft, &group, &fields);
    let mut submission = Submission {
        draft,
        ..Submission::default()
    };
    let intent = fields.get("intent").map_or("navigate", String::as_str);
    let result = async {
        // Native navigation must retain every selected image before rendering a
        // destination. Only Save selects these owned handles in persisted metadata.
        for (name, image) in uploads {
            let role = name.strip_prefix("upload_").ok_or(TextKey::UploadFailed)?;
            upload(&state, &mut submission, &group, role, Some(image)).await?;
        }
        if intent.starts_with("upload:") {
            Ok(())
        } else {
            execute(&state, &mut submission, &group, intent).await
        }
    }
    .await;
    if let Err(key) = result {
        submission.error_fields = match key {
            TextKey::Title => vec!["metadata_title".into()],
            TextKey::Year => vec!["metadata_year".into()],
            TextKey::UseChaptersAsMediaItems => vec!["chapter_input".into()],
            TextKey::BlockerSeasonEpisodeMissing | TextKey::NumberEpisodes => {
                if group.starts_with("item:") {
                    vec!["season".into(), "episode".into()]
                } else {
                    number_error_fields(&fields, intent)
                }
            }
            _ => Vec::new(),
        };
        let owner = Owner::new();
        submission.error = Some(owner.with(|| {
            provide_context(crate::admin::i18n::load_language_initial_for_state(&state));
            tr(key).to_owned()
        }));
    }
    let context_state = state.clone();
    let options = crate::host::config::leptos_configuration().leptos_options;
    parts.method = Method::GET;
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.remove(header::CONTENT_TYPE);
    let render = leptos_axum::render_app_to_stream_with_context(
        move || {
            provide_context(context_state.clone());
            provide_context(submission.clone());
        },
        move || crate::admin::app::shell(options.clone(), state.clone()),
    );
    let mut response = render(Request::from_parts(parts, Body::empty())).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

fn valid_group(group: &str) -> bool {
    matches!(
        group,
        "items" | "settings" | "playback" | "activity" | "artwork"
    ) || group
        .strip_prefix("item:")
        .or_else(|| group.strip_prefix("artwork:"))
        .and_then(|s| s.parse::<i64>().ok())
        .is_some_and(|id| id > 0)
}

fn merge(draft: &mut Draft, group: &str, fields: &Fields) {
    let stored = draft.groups.entry(group.to_owned()).or_default();
    let original = stored.clone();
    for (key, value) in fields {
        if !matches!(key.as_str(), "draft" | "group" | "intent")
            && !key.starts_with("baseline_")
            && fields.get(&format!("baseline_{key}")) != Some(&super::input_token(value))
        {
            let dirty_review = key
                .strip_prefix("review_snapshot_")
                .and_then(|id| id.parse::<i64>().ok())
                .map_or_else(
                    || {
                        [
                            "season",
                            "episode",
                            "numbering_mode",
                            "content_kind",
                            "included",
                            "liveness",
                        ]
                        .iter()
                        .any(|name| original.contains_key(*name))
                    },
                    |id| row_has_edits(&original, id),
                );
            if key.starts_with("review_snapshot") && dirty_review {
                stored.entry(key.clone()).or_insert_with(|| value.clone());
            } else {
                stored.insert(key.clone(), value.clone());
            }
        }
        if let Some(id) = key
            .strip_prefix("selected_")
            .and_then(|s| s.parse::<i64>().ok())
        {
            if value == "true" {
                draft.selected.insert(id);
            } else {
                draft.selected.remove(&id);
            }
        }
    }
    // The Items heading edits the list title in place. Its metadata fence has a
    // baseline, so it is retained only together with a retained title edit.
    if group == "items"
        && stored.contains_key("metadata_title")
        && !stored.contains_key("metadata_etag")
    {
        if let Some(etag) = fields.get("metadata_etag") {
            stored.insert("metadata_etag".into(), etag.clone());
        }
    }
    // Keep the radio choice with edited numbers, even when that choice itself
    // still matches the rendered baseline. Unnumbered must ignore stale inputs.
    if group.starts_with("item:")
        && ["season", "episode"].iter().any(|key| {
            fields.get(*key).is_some_and(|value| {
                fields.get(&format!("baseline_{key}")) != Some(&super::input_token(value))
            })
        })
    {
        if let Some(mode) = fields.get("numbering_mode") {
            stored.insert("numbering_mode".into(), mode.clone());
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Explicit native action dispatch shares one draft and revision fence"
)]
async fn execute(
    state: &AppState,
    submission: &mut Submission,
    group: &str,
    intent: &str,
) -> Result<()> {
    let d = &mut submission.draft;
    let fields = d.groups.get(group).cloned().unwrap_or_default();
    if d.category == "blockers"
        && matches!(
            intent,
            "number"
                | "classify"
                | "auto-number"
                | "clear-numbering"
                | "select-range"
                | "bulk-options"
        )
    {
        let readiness = state
            .application
            .addon_media_list_publication_readiness(d.list_id)
            .await
            .map_err(|_| TextKey::AddonWorkspaceLoadIncomplete)?;
        if readiness.saved_revision != d.revision {
            return Err(TextKey::AddonWorkspaceSaveConflict);
        }
        d.publication_blocker_items = readiness
            .blockers
            .iter()
            .filter_map(|blocker| blocker.item_id)
            .collect();
    }

    if d.stale_groups.contains(group)
        && matches!(
            intent,
            "save"
                | "number"
                | "classify"
                | "auto-number"
                | "clear-numbering"
                | "bulk-options"
                | "include-selected"
                | "exclude-selected"
                | "preview-options"
        )
    {
        return Err(TextKey::AddonWorkspaceRetained);
    }
    if matches!(
        intent,
        "number"
            | "classify"
            | "auto-number"
            | "clear-numbering"
            | "bulk-options"
            | "include-selected"
            | "exclude-selected"
    ) || intent.starts_with("start:")
        || intent.starts_with("range-start:")
    {
        d.expanded_bulk = true;
    }
    match intent {
        "navigate" => {}
        "review-current" => {
            let current = state
                .application
                .get_addon_media_list(d.list_id)
                .await
                .map_err(|_| TextKey::AddonWorkspaceRetained)?
                .ok_or(TextKey::AddonMediaListNotFound)?;
            if current.draft_revision != d.revision {
                d.stale_groups.extend(d.groups.keys().cloned());
            }
            d.revision = current.draft_revision;
            d.stale_groups.remove(group);
            if group == "playback" {
                let (revision, baseline) = state
                    .application
                    .ports()
                    .addon_media_lists()
                    .playback_baseline(d.list_id)
                    .await
                    .map_err(|error| save_error_key(&error.into()))?;
                if revision != d.revision {
                    return Err(TextKey::AddonWorkspaceRetained);
                }
                d.playback_baseline = Some(baseline);
            }
            d.option_undo = None;
            let target = d.groups.entry(group.into()).or_default();
            if matches!(group, "settings" | "artwork") {
                let nfo = state
                    .application
                    .get_addon_media_list_show_nfo(d.list_id)
                    .await
                    .map_err(|_| TextKey::AddonWorkspaceRetained)?;
                target.insert("metadata_etag".into(), nfo.etag);
            } else if let Ok(id) = item_id(group) {
                let detail = state
                    .application
                    .get_addon_media_list_item(d.list_id, id)
                    .await
                    .map_err(|_| TextKey::AddonWorkspaceRetained)?
                    .ok_or(TextKey::AddonMediaListNotFound)?;
                target.insert("metadata_etag".into(), detail.nfo.etag);
                target.insert(
                    "review_snapshot".into(),
                    serde_json::to_string(&review(&detail.item))
                        .map_err(|_| TextKey::AddonWorkspaceRetained)?,
                );
            } else if group == "items" {
                let rows = state
                    .application
                    .list_addon_media_list_items(d.list_id)
                    .await
                    .map_err(|_| TextKey::AddonWorkspaceRetained)?;
                for row in rows {
                    if let Some(snapshot) = target.get_mut(&format!("review_snapshot_{}", row.id)) {
                        *snapshot = serde_json::to_string(&review(&row))
                            .map_err(|_| TextKey::AddonWorkspaceRetained)?;
                    }
                }
            }
            submission.notice = Some(TextKey::AddonWorkspaceReviewRetained);
        }
        "cancel-confirmation" => {
            d.confirmation = None;
        }
        "undo" => {
            if let Some(previous) = d.undo.take() {
                if let Some(items) = previous.get("items") {
                    d.groups.insert("items".into(), items.clone());
                } else {
                    d.groups.remove("items");
                }
            }
        }
        "discard" => {
            d.groups.remove(group);
            if group == "items" {
                d.undo = None;
            }
        }
        "clear-selection" => {
            d.selected.clear();
        }
        "select-visible" | "deselect-visible" => {
            let list = state
                .application
                .get_addon_media_list(d.list_id)
                .await
                .map_err(|_| TextKey::AddonWorkspaceRetained)?
                .ok_or(TextKey::AddonMediaListNotFound)?;
            let rows: Vec<crate::admin::server_fns::addon_media_lists::AddonMediaListItemRow> =
                state
                    .application
                    .list_addon_media_list_items(d.list_id)
                    .await
                    .map_err(|_| TextKey::AddonWorkspaceRetained)?
                    .into_iter()
                    .map(Into::into)
                    .collect();
            let diff = state
                .application
                .addon_media_list_saved_draft_diff(d.list_id)
                .await
                .map(crate::admin::server_fns::addon_media_lists::AddonMediaListDraftDiffRow::from)
                .map_err(|_| TextKey::AddonWorkspaceRetained)?;
            let filtered = super::sorted_filtered(d, &rows, list.presentation_kind, &diff);
            let size = if d.page_size == 0 {
                filtered.len().max(1)
            } else {
                d.page_size
            };
            let page = d.page.min(filtered.len().div_ceil(size).saturating_sub(1));
            for row in filtered.into_iter().skip(page * size).take(size) {
                if intent == "deselect-visible" {
                    d.selected.remove(&row.id);
                } else if row.included
                    && d.field(
                        "items",
                        &format!("included_{}", row.id),
                        row.included.to_string(),
                    ) == "true"
                {
                    d.selected.insert(row.id);
                }
            }
        }
        "select-range" => bulk::select_range(state, d, &fields).await?,
        "filter" => {
            field(&fields, "query").clone_into(&mut d.query);
            field(&fields, "category").clone_into(&mut d.category);
            d.page = 0;
        }
        intent if intent.starts_with("sort:") => {
            let sort = &intent[5..];
            if !super::valid_sort(sort) {
                return Err(TextKey::AddonWorkspaceSort);
            }
            sort.clone_into(&mut d.sort);
            d.page = 0;
        }
        "clear-filter" => {
            d.query.clear();
            d.category.clear();
            d.page = 0;
            if let Some(items) = d.groups.get_mut("items") {
                items.remove("query");
                items.remove("category");
            }
        }
        "page-size" => {
            let size = field(&fields, "page_size")
                .parse()
                .map_err(|_| TextKey::AddonWorkspacePageSize)?;
            if ![0, 10, 25, 50, 100].contains(&size) {
                return Err(TextKey::AddonWorkspacePageSize);
            }
            d.page_size = size;
            d.page = 0;
        }
        "number" | "classify" | "auto-number" | "clear-numbering" | "include-selected"
        | "exclude-selected" => {
            preview_bulk(state, d, &fields, intent).await?;
            submission.focus_bulk_preview = true;
        }
        "save" => {
            let (command, mut undo) = save_command(state, d, group, &fields).await?;
            let review_undo = saved_review_undo(state, &command, group).await?;
            let revision = state
                .application
                .save_addon_media_list_editor_task(command)
                .await
                .map_err(|error| save_error_key(&error))?;
            for receipt in &mut undo {
                receipt.expected_revision = revision;
            }
            if group == "playback" {
                d.playback_baseline = None;
            }
            d.revision = revision;
            d.saved_review_undo = review_undo;
            d.option_undo = if undo.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&undo).map_err(|_| TextKey::AddonOptionsConflict)?)
            };
            refresh_related_metadata_fence(state, d, group, field(&fields, "metadata_etag")).await;
            d.groups.remove(group);
            if group == "items" {
                d.undo = None;
                let controls = fields
                    .into_iter()
                    .filter(|(key, _)| {
                        key.starts_with("bulk_")
                            || matches!(
                                key.as_str(),
                                "start_item"
                                    | "end_item"
                                    | "replace_existing"
                                    | "season_only"
                                    | "minimum_length"
                                    | "maximum_length"
                                    | "include_unknown_length"
                                    | "gap_days"
                                    | "detect_season"
                                    | "season_per_year"
                                    | "option_targets"
                            )
                    })
                    .collect();
                d.groups.insert("items".into(), controls);
            }
            submission.notice = Some(TextKey::Saved);
        }
        "undo-saved-review" => {
            let command = d
                .saved_review_undo
                .clone()
                .ok_or(TextKey::AddonWorkspaceRetained)?;
            if command.list_id != d.list_id || command.expected_revision != d.revision {
                return Err(TextKey::AddonWorkspaceRetained);
            }
            d.revision = state
                .application
                .save_addon_media_list_editor_task(command)
                .await
                .map_err(|error| save_error_key(&error))?;
            d.saved_review_undo = None;
            d.option_undo = None;
            submission.notice = Some(TextKey::Saved);
        }
        "undo-options" => {
            let receipts: Vec<ersatzrs_app::addon_media_list_options::AddonListOptionUndo> =
                serde_json::from_str(
                    d.option_undo
                        .as_deref()
                        .ok_or(TextKey::AddonOptionsConflict)?,
                )
                .map_err(|_| TextKey::AddonOptionsConflict)?;
            d.revision = state
                .application
                .undo_addon_list_option_task(receipts.clone())
                .await
                .map_err(|_| TextKey::AddonOptionsConflict)?;
            if let (Some(baseline), Some(first)) = (&mut d.playback_baseline, receipts.first()) {
                // Undo changes only these saved options. Keep the original
                // delivery, installation and warning fences for retained input.
                baseline.schema.clone_from(&first.previous_schema);
                for receipt in &receipts {
                    for previous in &receipt.previous {
                        if previous.item_id.is_none() {
                            if let Some(value) = &previous.value {
                                baseline
                                    .list_values
                                    .insert(receipt.key.clone(), value.clone());
                            } else {
                                baseline.list_values.remove(&receipt.key);
                            }
                        }
                    }
                }
            }
            d.option_undo = None;
        }
        "bulk-options" => bulk::select_options(state, d, &fields).await?,
        "preview-options" => {
            let ids = if group == "items" {
                crate::admin::server_fns::addon_list_options::parse_option_items(field(
                    &fields,
                    "option_targets",
                ))
                .map_err(|_| TextKey::Scope)?
            } else if group.starts_with("item:") {
                vec![item_id(group)?]
            } else {
                Vec::new()
            };
            for edit in option_edits(state, d, &fields, ids).await? {
                submission.option_previews.push(
                    state
                        .application
                        .preview_addon_list_option_edit(edit)
                        .await
                        .map_err(|_| TextKey::AddonOptionsConflict)?,
                );
            }
        }
        "publish" => {
            let outcome = state
                .application
                .publish_addon_media_list(ersatzrs_app::commands::PublishAddonMediaList {
                    id: d.list_id,
                    saved_revision: d.revision,
                })
                .await
                .map_err(|_| TextKey::PublishBlocked)?;
            // Preparation can retain the add-on's option defaults, which saves a
            // new revision. Follow exactly that returned revision so later saves
            // and publication use it; unsaved task input in the draft is kept.
            let (notice, saved_revision) = match outcome {
                ersatzrs_app::commands::PublishAddonMediaListOutcome::Queued {
                    saved_revision,
                    ..
                } => (TextKey::PublicationQueued, saved_revision),
                ersatzrs_app::commands::PublishAddonMediaListOutcome::Preparing {
                    saved_revision,
                    ..
                }
                | ersatzrs_app::commands::PublishAddonMediaListOutcome::PreparingMedia {
                    saved_revision,
                    ..
                } => (TextKey::PreparingPublication, saved_revision),
            };
            d.revision = saved_revision;
            submission.notice = Some(notice);
        }
        "test-source" => {
            state
                .application
                .test_addon_media_list_source(ersatzrs_app::commands::TestAddonMediaListSource {
                    id: d.list_id,
                    saved_revision: d.revision,
                })
                .await
                .map_err(|_| TextKey::TestSource)?;
            submission.notice = Some(TextKey::SourceTestQueued);
        }
        "reconcile" => {
            state
                .application
                .reconcile_addon_media_list(ersatzrs_app::commands::ReconcileAddonMediaList {
                    id: d.list_id,
                })
                .await
                .map_err(|_| TextKey::ReconcileEntireSource)?;
            submission.notice = Some(TextKey::ReconcileQueued);
        }
        "recover-retry" | "recover-restore" => {
            state
                .application
                .recover_addon_media_list_publication(
                    ersatzrs_app::commands::RecoverAddonMediaListPublication {
                        list_id: d.list_id,
                        job_id: parse(&fields, "job_id")?,
                        generation: parse(&fields, "generation")?,
                        action: if intent == "recover-retry" {
                            AddonMediaListRecoveryAction::Retry
                        } else {
                            AddonMediaListRecoveryAction::RestorePreviousRevision
                        },
                    },
                )
                .await
                .map_err(|_| TextKey::PublishBlocked)?;
        }
        "restore-chapters" => {
            d.groups
                .entry(group.to_owned())
                .or_default()
                .insert("chapter_input".into(), String::new());
        }
        "fetch-details" => {
            let id = item_id(group)?;
            state
                .application
                .request_addon_media_list_item_details(
                    ersatzrs_app::commands::RequestAddonMediaListItemDetails {
                        list_id: d.list_id,
                        item_id: id,
                    },
                )
                .await
                .map_err(|_| TextKey::FetchDetails)?;
        }
        _ if intent.starts_with("page:") => {
            d.page = intent[5..]
                .parse()
                .map_err(|_| TextKey::AddonWorkspacePageSize)?;
        }
        _ if intent.starts_with("clear-row:") => {
            let id = intent[10..].parse::<i64>().map_err(|_| TextKey::NoItems)?;
            state
                .application
                .get_addon_media_list_item(d.list_id, id)
                .await
                .map_err(|_| TextKey::NoItems)?
                .ok_or(TextKey::NoItems)?;
            d.undo = Some(d.groups.clone());
            let fields = d.groups.entry("items".into()).or_default();
            fields.insert(format!("season_{id}"), String::new());
            fields.insert(format!("episode_{id}"), String::new());
            submission.focus_bulk_preview = true;
        }
        _ if intent.starts_with("range-start:") => {
            let id = intent[12..].parse::<i64>().map_err(|_| TextKey::Range)?;
            state
                .application
                .get_addon_media_list_item(d.list_id, id)
                .await
                .map_err(|_| TextKey::NoItems)?
                .ok_or(TextKey::NoItems)?;
            let fields = d.groups.entry("items".into()).or_default();
            fields.insert("start_item".into(), id.to_string());
            fields.insert("end_item".into(), id.to_string());
            fields.insert("bulk_scope".into(), "matching".into());
        }
        _ if intent.starts_with("start:") => {
            let id = intent[6..]
                .parse::<i64>()
                .map_err(|_| TextKey::NumberEpisodes)?;
            let fields = d.groups.entry("items".into()).or_default();
            fields.insert("start_item".into(), id.to_string());
            fields.insert("bulk_scope".into(), "matching".into());
        }
        _ => lifecycle::execute(state, submission, group, intent).await?,
    }
    Ok(())
}

fn field<'a>(fields: &'a Fields, name: &str) -> &'a str {
    fields.get(name).map(String::as_str).unwrap_or_default()
}
fn parse<T: std::str::FromStr>(fields: &Fields, name: &str) -> Result<T> {
    field(fields, name)
        .parse()
        .map_err(|_| TextKey::AddonWorkspaceRetained)
}
fn enum_value<T: serde::de::DeserializeOwned>(fields: &Fields, name: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(field(fields, name).to_owned()))
        .map_err(|_| TextKey::AddonWorkspaceRetained)
}
fn number(fields: &Fields, name: &str) -> Result<Option<i32>> {
    if field(fields, name).trim().is_empty() {
        Ok(None)
    } else {
        let n = parse(fields, name)?;
        if n < 0 {
            return Err(TextKey::NumberEpisodes);
        }
        Ok(Some(n))
    }
}
fn item_id(group: &str) -> Result<i64> {
    group
        .strip_prefix("item:")
        .or_else(|| group.strip_prefix("artwork:"))
        .and_then(|s| s.parse().ok())
        .ok_or(TextKey::AddonMediaListNotFound)
}

fn save_error_key(error: &ersatzrs_app::ApplicationError) -> TextKey {
    use ersatzrs_app::ApplicationError;
    match error {
        ApplicationError::Conflict(_) => TextKey::AddonWorkspaceSaveConflict,
        ApplicationError::Validation { .. } | ApplicationError::InvalidData { .. } => {
            TextKey::AddonWorkspaceSaveInvalid
        }
        ApplicationError::NotFound { .. } => TextKey::AddonMediaListNotFound,
        _ => TextKey::AddonWorkspaceSaveStorage,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Each task projects a distinct field group into the same atomic command"
)]
async fn save_command(
    state: &AppState,
    d: &Draft,
    group: &str,
    fields: &Fields,
) -> Result<(
    AddonMediaListEditorSave,
    Vec<ersatzrs_app::addon_media_list_options::AddonListOptionUndo>,
)> {
    let mut undo = Vec::new();
    let edits_metadata = fields
        .keys()
        .any(|key| key.starts_with("metadata_") && key != "metadata_etag");
    let mut save = AddonMediaListEditorSave {
        list_id: d.list_id,
        expected_revision: d.revision,
        playback_baseline: None,
        metadata: None,
        settings: None,
        delivery_mode: None,
        disk_warning_gb: None,
        options: None,
        items: Vec::new(),
    };
    match group {
        _ if group == "settings" || group.starts_with("artwork") => {
            if let Some(id) = group
                .strip_prefix("artwork:")
                .and_then(|v| v.parse::<i64>().ok())
            {
                let current = state
                    .application
                    .get_addon_media_list_item(d.list_id, id)
                    .await
                    .map_err(|_| TextKey::EditMetadata)?
                    .ok_or(TextKey::AddonMediaListNotFound)?;
                save.items.push(AddonMediaListEditorItem {
                    review: review(&current.item),
                    expected_review: None,
                    metadata: Some(metadata(fields, current.nfo.metadata)?),
                    role: None,
                    chapters: None,
                    replacement: None,
                });
            } else {
                let current = state
                    .application
                    .get_addon_media_list_show_nfo(d.list_id)
                    .await
                    .map_err(|_| TextKey::EditMetadata)?;
                if edits_metadata || (group == "settings" && !current.conflicts.is_empty()) {
                    save.metadata = Some(metadata(fields, current.metadata)?);
                }
            }
            if group == "settings"
                && [
                    "refresh_cadence",
                    "refresh_publication_policy",
                    "load_item_details",
                    "generate_playlist",
                    "presentation_kind",
                ]
                .iter()
                .any(|key| fields.contains_key(*key))
            {
                let current = state
                    .application
                    .get_addon_media_list(d.list_id)
                    .await
                    .map_err(|_| TextKey::AddonWorkspaceRetained)?
                    .ok_or(TextKey::AddonMediaListNotFound)?;
                save.settings = Some(AddonMediaListEditorSettings {
                    refresh_cadence: if fields.contains_key("refresh_cadence") {
                        enum_value(fields, "refresh_cadence")?
                    } else {
                        current.refresh_cadence
                    },
                    refresh_publication_policy: if fields.contains_key("refresh_publication_policy")
                    {
                        enum_value(fields, "refresh_publication_policy")?
                    } else {
                        current.refresh_publication_policy
                    },
                    detail_policy: if !fields.contains_key("load_item_details") {
                        current.detail_import_policy
                    } else if field(fields, "load_item_details") == "true" {
                        AddonMediaListDetailPolicy::Automatic
                    } else {
                        AddonMediaListDetailPolicy::ManualOnly
                    },
                    generate_playlist: fields
                        .get("generate_playlist")
                        .map_or(current.generate_playlist, |v| v == "true"),
                    presentation_kind: if fields.contains_key("presentation_kind") {
                        enum_value(fields, "presentation_kind")?
                    } else {
                        current.requested_presentation_kind
                    },
                });
            }
        }
        "playback" => {
            save.delivery_mode = Some(enum_value(fields, "delivery_mode")?);
            save.disk_warning_gb = fields
                .get("disk_warning_gb")
                .map(|_| parse(fields, "disk_warning_gb"))
                .transpose()?;
            let mut validation_draft = d.clone();
            if let Some(expected) = &d.playback_baseline {
                let (revision, current) = state
                    .application
                    .ports()
                    .addon_media_lists()
                    .playback_baseline(d.list_id)
                    .await
                    .map_err(|error| save_error_key(&error.into()))?;
                if expected != &current {
                    return Err(TextKey::AddonWorkspaceSaveConflict);
                }
                validation_draft.revision = revision;
                save.playback_baseline = Some(expected.clone());
            }
            (save.options, undo) = options(state, &validation_draft, fields, Vec::new()).await?;
        }
        "items" => {
            if fields.contains_key("metadata_title") {
                // Only the title is edited here; every other list metadata
                // field keeps its saved value under the same fence.
                let current = state
                    .application
                    .get_addon_media_list_show_nfo(d.list_id)
                    .await
                    .map_err(|_| TextKey::EditMetadata)?;
                let title = Fields::from([
                    (
                        "metadata_title".into(),
                        field(fields, "metadata_title").to_owned(),
                    ),
                    (
                        "metadata_etag".into(),
                        field(fields, "metadata_etag").to_owned(),
                    ),
                ]);
                save.metadata = Some(metadata(&title, current.metadata)?);
            }
            if let Some(value) = fields.get("option_targets") {
                let ids = crate::admin::server_fns::addon_list_options::parse_option_items(value)
                    .map_err(|_| TextKey::Scope)?;
                if ids.is_empty() {
                    return Err(TextKey::Scope);
                }
                (save.options, undo) = options(state, d, fields, ids).await?;
            }
            let rows = state
                .application
                .list_addon_media_list_items(d.list_id)
                .await
                .map_err(|_| TextKey::NoItems)?;
            let owned = rows
                .iter()
                .map(|row| row.id)
                .collect::<std::collections::BTreeSet<_>>();
            for key in fields.keys() {
                if key == "season_per_year" {
                    continue;
                }
                for prefix in ["season_", "episode_", "kind_", "liveness_", "included_"] {
                    if let Some(value) = key.strip_prefix(prefix) {
                        if value == "only" {
                            continue;
                        }
                        let id = value
                            .parse::<i64>()
                            .map_err(|_| TextKey::AddonMediaListNotFound)?;
                        if !owned.contains(&id) {
                            return Err(TextKey::AddonMediaListNotFound);
                        }
                    }
                }
            }
            for row in rows {
                let season_name = format!("season_{}", row.id);
                let episode_name = format!("episode_{}", row.id);
                let kind_name = format!("kind_{}", row.id);
                let live_name = format!("liveness_{}", row.id);
                let included_name = format!("included_{}", row.id);
                if fields.contains_key(&season_name)
                    || fields.contains_key(&episode_name)
                    || fields.contains_key(&kind_name)
                    || fields.contains_key(&live_name)
                    || fields.contains_key(&included_name)
                {
                    let mut review = review(&row);
                    if fields.contains_key(&included_name) {
                        review.included = parse::<bool>(fields, &included_name)?;
                    }
                    if fields.contains_key(&season_name) {
                        review.season = number(fields, &season_name)?;
                    }
                    if fields.contains_key(&episode_name) {
                        review.episode = number(fields, &episode_name)?;
                    }
                    if fields.contains_key(&kind_name) {
                        review.content_kind = enum_value(fields, &kind_name)?;
                    }
                    if fields.contains_key(&live_name) {
                        review.liveness_override = liveness(fields, &live_name)?;
                    }
                    save.items.push(AddonMediaListEditorItem {
                        review,
                        expected_review: fields
                            .get(&format!("review_snapshot_{}", row.id))
                            .map(|v| {
                                serde_json::from_str(v).map_err(|_| TextKey::AddonWorkspaceRetained)
                            })
                            .transpose()?,
                        metadata: None,
                        role: None,
                        chapters: None,
                        replacement: None,
                    });
                }
            }
        }
        _ => {
            let id = item_id(group)?;
            let current = state
                .application
                .get_addon_media_list_item(d.list_id, id)
                .await
                .map_err(|_| TextKey::EditMetadata)?
                .ok_or(TextKey::AddonMediaListNotFound)?;
            let mut edit = review(&current.item);
            (edit.season, edit.episode) = item_numbering(fields, (edit.season, edit.episode))?;
            if fields.contains_key("content_kind") {
                edit.content_kind = enum_value(fields, "content_kind")?;
            }
            if fields.contains_key("liveness") {
                edit.liveness_override = liveness(fields, "liveness")?;
            }
            if fields.contains_key("included") {
                edit.included = field(fields, "included") == "true";
            }
            let chapters = fields
                .get("chapter_input")
                .filter(|text| {
                    current.item.parent_provider_id.is_none()
                        && !(current.item.chapter_input.trim().is_empty()
                            && text.trim() == current.item.provider_chapter_input.trim())
                        && text.trim() != current.item.chapter_input.trim()
                })
                .cloned();
            if let Some(text) = &chapters {
                ersatzrs_core::media_chapters::parse_chapter_input(
                    text,
                    current.item.duration_seconds,
                )
                .map_err(|_| TextKey::UseChaptersAsMediaItems)?;
            }
            save.items.push(AddonMediaListEditorItem {
                review: edit,
                expected_review: fields
                    .get("review_snapshot")
                    .map(|v| serde_json::from_str(v).map_err(|_| TextKey::AddonWorkspaceRetained))
                    .transpose()?,
                metadata: if edits_metadata || !current.nfo.conflicts.is_empty() {
                    Some(metadata(fields, current.nfo.metadata)?)
                } else {
                    None
                },
                role: fields
                    .get("role")
                    .map(|_| enum_value(fields, "role"))
                    .transpose()?,
                chapters,
                replacement: fields
                    .get("replacement")
                    .map(|value| {
                        if value.is_empty() {
                            Ok(None)
                        } else {
                            let (media, version) =
                                value.split_once(':').ok_or(TextKey::InvalidReplacement)?;
                            Ok(Some((
                                media.parse().map_err(|_| TextKey::InvalidReplacement)?,
                                version.parse().map_err(|_| TextKey::InvalidReplacement)?,
                            )))
                        }
                    })
                    .transpose()?,
            });
            (save.options, undo) = options(state, d, fields, vec![id]).await?;
        }
    }
    if group.starts_with("item:") {
        let original = state
            .application
            .get_addon_media_list_item(d.list_id, item_id(group)?)
            .await
            .map_err(|_| TextKey::AddonMediaListNotFound)?
            .ok_or(TextKey::AddonMediaListNotFound)?;
        save.items.retain(|item| {
            item.review != review(&original.item)
                || item.metadata.is_some()
                || item.chapters.is_some()
                || item.replacement.is_some()
                || item
                    .role
                    .is_some_and(|role| role != original.item.collection_role)
        });
    }
    for item in &save.items {
        if item.review.season.is_some() != item.review.episode.is_some() {
            return Err(TextKey::BlockerSeasonEpisodeMissing);
        }
    }
    Ok((save, undo))
}

fn item_numbering(
    fields: &Fields,
    saved: (Option<i32>, Option<i32>),
) -> Result<(Option<i32>, Option<i32>)> {
    let mode = fields.get("numbering_mode").map(String::as_str);
    if mode == Some("none") {
        return Ok((None, None));
    }
    if mode.is_some_and(|mode| mode != "numbered") {
        return Err(TextKey::NumberEpisodes);
    }
    let season = if fields.contains_key("season") {
        number(fields, "season")?
    } else {
        saved.0
    };
    let episode = if fields.contains_key("episode") {
        number(fields, "episode")?
    } else {
        saved.1
    };
    if season.is_some() != episode.is_some() || (mode == Some("numbered") && season.is_none()) {
        return Err(TextKey::BlockerSeasonEpisodeMissing);
    }
    Ok((season, episode))
}

fn review(row: &AddonMediaListItemAdminRow) -> AddonMediaListItemReviewEdit {
    AddonMediaListItemReviewEdit {
        item_id: row.id,
        content_kind: row.content_kind,
        included: row.included,
        liveness_override: row.liveness_override,
        season: row.season,
        episode: row.episode,
    }
}
fn liveness(fields: &Fields, key: &str) -> Result<Option<RemoteStreamLiveness>> {
    if field(fields, key) == "inherit" {
        Ok(None)
    } else {
        enum_value(fields, key).map(Some)
    }
}

fn metadata(
    fields: &Fields,
    mut current: AddonMediaListNfoMetadata,
) -> Result<AddonMediaListEditorMetadata> {
    let mut json = serde_json::to_value(&current).map_err(|_| TextKey::EditMetadata)?;
    for name in [
        "title",
        "original_title",
        "sort_title",
        "plot",
        "release_date",
    ] {
        if let Some(text) = fields.get(&format!("metadata_{name}")) {
            json[name] = if text.trim().is_empty() && name != "title" {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(text.trim().into())
            };
        }
    }
    if fields.contains_key("metadata_year") {
        json["year"] =
            serde_json::to_value(number(fields, "metadata_year").map_err(|_| TextKey::Year)?)
                .map_err(|_| TextKey::Year)?;
    }
    for name in [
        "genres",
        "tags",
        "studios",
        "directors",
        "writers",
        "artists",
        "languages",
    ] {
        if let Some(text) = fields.get(&format!("metadata_{name}")) {
            json[name] = serde_json::json!(text
                .split(if name == "tags" { '\n' } else { ',' })
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>());
        }
    }
    current = serde_json::from_value(json).map_err(|_| TextKey::EditMetadata)?;
    if fields.contains_key("metadata_title") && current.title.trim().is_empty() {
        return Err(TextKey::Title);
    }
    if let Some(text) = fields.get("metadata_people") {
        let mut web: crate::admin::server_fns::addon_media_lists::AddonMediaListNfoMetadata =
            current.into();
        web.people = super::super::people_values(text, &web.people);
        current = web.into();
    }
    for (name, role) in [
        ("poster", AddonMediaListArtworkRole::Poster),
        ("fanart", AddonMediaListArtworkRole::Fanart),
        ("thumbnail", AddonMediaListArtworkRole::Thumb),
    ] {
        if let Some(url) = fields.get(&format!("metadata_{name}")) {
            if current
                .artwork
                .iter()
                .find(|a| a.role == role)
                .map(|a| a.url.as_str())
                .unwrap_or_default()
                == url.trim()
            {
                continue;
            }
            let selected = current
                .artwork
                .iter()
                .find(|a| a.role == role && a.url == url.trim())
                .cloned();
            current.artwork.retain(|a| a.role != role);
            if !url.trim().is_empty() {
                current
                    .artwork
                    .push(selected.unwrap_or(AddonMediaListArtwork {
                        url: url.trim().into(),
                        role,
                        width: None,
                        height: None,
                        language: None,
                    }));
            }
        }
    }
    Ok(AddonMediaListEditorMetadata {
        metadata: current,
        etag: field(fields, "metadata_etag").to_owned(),
    })
}

async fn option_edits(
    state: &AppState,
    d: &Draft,
    fields: &Fields,
    ids: Vec<i64>,
) -> Result<Vec<ersatzrs_app::addon_media_list_options::AddonListOptionEdit>> {
    let editor =
        crate::admin::server_fns::addon_list_options::load_option_editor(state, d.list_id, ids)
            .await
            .map_err(|error| save_error_key(&error))?;
    let Some(editor) = editor else {
        return Ok(Vec::new());
    };
    let mut edits = Vec::new();
    for field in editor.fields {
        if let Some(value) = fields.get(&format!("option_{}", field.definition.key)) {
            if value == "__unchanged" {
                continue;
            }
            let mut edit = field.edit;
            edit.expected_revision = d.revision;
            edit.value = if value.is_empty() {
                None
            } else {
                Some(serde_json::from_str(value).map_err(|_| TextKey::AddonWorkspaceSaveInvalid)?)
            };
            if editor.provisional || field.rows.iter().any(|r| r.before != edit.value) {
                edits.push(edit);
            }
        }
    }
    Ok(edits)
}

async fn options(
    state: &AppState,
    d: &Draft,
    fields: &Fields,
    ids: Vec<i64>,
) -> Result<(
    Option<AddonMediaListDraftOptions>,
    Vec<ersatzrs_app::addon_media_list_options::AddonListOptionUndo>,
)> {
    let edits = option_edits(state, d, fields, ids).await?;
    state
        .application
        .prepare_addon_list_option_edits(edits)
        .await
        .map_err(|error| save_error_key(&error))
}

async fn upload(
    state: &AppState,
    submission: &mut Submission,
    group: &str,
    name: &str,
    image: Option<(String, axum::body::Bytes)>,
) -> Result<()> {
    let d = &mut submission.draft;
    if !group.starts_with("artwork") {
        return Err(TextKey::UploadFailed);
    }
    let role = match name {
        "poster" => AddonMediaListArtworkRole::Poster,
        "fanart" => AddonMediaListArtworkRole::Fanart,
        "thumbnail" => AddonMediaListArtworkRole::Thumb,
        _ => return Err(TextKey::UploadFailed),
    };
    let (content_type, bytes) = image.ok_or(TextKey::UploadFailed)?;
    let item_id = group
        .strip_prefix("artwork:")
        .map(str::parse::<i64>)
        .transpose()
        .map_err(|_| TextKey::UploadFailed)?;
    let url = state
        .application
        .ports()
        .addon_media_lists()
        .stage_editor_artwork(d.list_id, item_id, role, &content_type, &bytes)
        .await
        .map_err(|_| TextKey::UploadFailed)?;
    d.groups
        .entry(group.into())
        .or_default()
        .insert(format!("metadata_{name}"), url);
    submission.notice = Some(TextKey::UnsavedChanges);
    Ok(())
}

fn size_error(state: &AppState) -> Response {
    let owner = Owner::new();
    let html=owner.with(|| {
        provide_context(crate::admin::i18n::load_language_initial_for_state(state));
        view!{<!DOCTYPE html><html><head><meta name="viewport" content="width=device-width, initial-scale=1"/><title>{tr(TextKey::AddonWorkspaceSizeLimit)}</title></head><body><main><h1>{tr(TextKey::AddonWorkspaceSizeLimit)}</h1><p>"8 MiB · "{tr(TextKey::AddonWorkspacePageSize)}</p></main></body></html>}.to_html()
    });
    (StatusCode::PAYLOAD_TOO_LARGE, axum::response::Html(html)).into_response()
}

async fn saved_review_undo(
    state: &AppState,
    command: &AddonMediaListEditorSave,
    group: &str,
) -> Result<Option<AddonMediaListEditorSave>> {
    if group != "items" || command.items.is_empty() {
        return Ok(None);
    }
    let rows = state
        .application
        .list_addon_media_list_items(command.list_id)
        .await
        .map_err(|_| TextKey::AddonWorkspaceRetained)?;
    let items = command
        .items
        .iter()
        .map(|changed| {
            let row = rows
                .iter()
                .find(|row| row.id == changed.review.item_id)
                .ok_or(TextKey::AddonWorkspaceRetained)?;
            Ok(AddonMediaListEditorItem {
                review: AddonMediaListItemReviewEdit {
                    item_id: row.id,
                    content_kind: row.content_kind,
                    included: row.included,
                    liveness_override: row.liveness_override,
                    season: row.season,
                    episode: row.episode,
                },
                expected_review: Some(changed.review),
                role: None,
                metadata: None,
                chapters: None,
                replacement: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(AddonMediaListEditorSave {
        list_id: command.list_id,
        playback_baseline: None,
        expected_revision: command
            .expected_revision
            .checked_add(1)
            .ok_or(TextKey::AddonWorkspaceRetained)?,
        metadata: None,
        settings: None,
        delivery_mode: None,
        disk_warning_gb: None,
        options: None,
        items,
    }))
}

fn number_error_fields(fields: &Fields, intent: &str) -> Vec<String> {
    let mut invalid = Vec::new();
    for key in fields.keys().filter(|k| {
        k.strip_prefix("season_")
            .is_some_and(|s| s.parse::<i64>().is_ok())
    }) {
        let episode = key.replacen("season_", "episode_", 1);
        let pair = (number(fields, key), number(fields, &episode));
        if !matches!(pair, (Ok(None), Ok(None)) | (Ok(Some(_)), Ok(Some(_)))) {
            invalid.extend([key.clone(), episode]);
        }
    }
    if !invalid.is_empty() {
        invalid
    } else if intent == "auto-number" {
        vec!["detect_season".into()]
    } else {
        vec!["bulk_season".into(), "bulk_episode".into()]
    }
}

fn row_has_edits(fields: &Fields, id: i64) -> bool {
    ["season", "episode", "kind", "liveness", "included"]
        .iter()
        .any(|name| fields.contains_key(&format!("{name}_{id}")))
}

async fn refresh_related_metadata_fence(
    state: &AppState,
    d: &mut Draft,
    group: &str,
    previous: &str,
) {
    let (paired, etag) = if matches!(group, "settings" | "artwork" | "items") {
        (
            // List-level metadata is edited on List settings, list artwork and
            // the Items heading; each retained fence follows a save of another.
            ["settings", "artwork", "items"]
                .into_iter()
                .filter(|other| *other != group)
                .map(str::to_owned)
                .collect::<Vec<_>>(),
            state
                .application
                .get_addon_media_list_show_nfo(d.list_id)
                .await
                .map(|form| Some(form.etag)),
        )
    } else if let Ok(id) = item_id(group) {
        (
            vec![if group.starts_with("item:") {
                format!("artwork:{id}")
            } else {
                format!("item:{id}")
            }],
            state
                .application
                .get_addon_media_list_item(d.list_id, id)
                .await
                .map(|detail| detail.map(|detail| detail.nfo.etag)),
        )
    } else {
        return;
    };
    let Ok(Some(etag)) = etag else {
        // The task is already saved. Keep the paired draft, but require an
        // explicit review when its refreshed concurrency fence is unavailable.
        tracing::warn!("Saved add-on task metadata fence could not be refreshed");
        for paired in paired {
            if d.groups.contains_key(&paired) {
                d.stale_groups.insert(paired);
            }
        }
        return;
    };
    for paired in paired {
        if let Some(fields) = d.groups.get_mut(&paired) {
            if fields
                .get("metadata_etag")
                .is_some_and(|retained| retained == previous)
            {
                fields.insert("metadata_etag".into(), etag.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_numbering_retains_native_mode_and_validates_pairs() {
        let mut draft = Draft::default();
        let fields = Fields::from([
            ("numbering_mode".into(), "none".into()),
            (
                "baseline_numbering_mode".into(),
                super::super::input_token("none"),
            ),
            ("season".into(), "2".into()),
            ("episode".into(), "7".into()),
        ]);
        merge(&mut draft, "item:1", &fields);
        assert_eq!(
            item_numbering(&draft.groups["item:1"], (Some(1), Some(3))).unwrap(),
            (None, None)
        );
        let mut fields = fields;
        fields.insert("numbering_mode".into(), "numbered".into());
        merge(&mut draft, "item:1", &fields);
        assert_eq!(
            item_numbering(&draft.groups["item:1"], (None, None)).unwrap(),
            (Some(2), Some(7))
        );
        fields.insert("episode".into(), String::new());
        assert!(item_numbering(&fields, (None, None)).is_err());
        fields.insert("season".into(), String::new());
        assert!(item_numbering(&fields, (None, None)).is_err());
        fields.remove("numbering_mode");
        assert_eq!(
            item_numbering(&fields, (Some(1), Some(3))).unwrap(),
            (None, None)
        );
    }

    #[test]
    fn items_title_edit_retains_its_metadata_fence_only_with_the_edit() {
        let token = super::super::input_token;
        let posted = |title: &str| {
            Fields::from([
                ("metadata_title".into(), title.into()),
                ("baseline_metadata_title".into(), token("Saved list")),
                ("metadata_etag".into(), "etag-1".into()),
                ("baseline_metadata_etag".into(), token("etag-1")),
            ])
        };
        let mut draft = Draft::default();
        merge(&mut draft, "items", &posted("Saved list"));
        let items = &draft.groups["items"];
        assert!(!items.contains_key("metadata_title"));
        assert!(
            !items.contains_key("metadata_etag"),
            "an unedited title keeps no fence"
        );

        merge(&mut draft, "items", &posted("Renamed list"));
        let items = &draft.groups["items"];
        assert_eq!(items["metadata_title"], "Renamed list");
        assert_eq!(
            items["metadata_etag"], "etag-1",
            "the fence the title was edited against"
        );
    }

    #[test]
    fn native_navigation_retains_only_changed_input_and_keeps_original_row_fence() {
        let mut draft = Draft::default();
        let mut fields = Fields::from([
            ("metadata_title".into(), "Saved title".into()),
            (
                "baseline_metadata_title".into(),
                super::super::input_token("Saved title"),
            ),
            ("metadata_etag".into(), "etag".into()),
            ("review_snapshot".into(), "original".into()),
        ]);
        merge(&mut draft, "item:1", &fields);
        assert!(!draft.groups["item:1"].contains_key("metadata_title"));
        assert_eq!(draft.groups["item:1"]["metadata_etag"], "etag");
        fields.insert("metadata_title".into(), "Unsaved title".into());
        merge(&mut draft, "item:1", &fields);
        fields.insert(
            "baseline_metadata_title".into(),
            super::super::input_token("Unsaved title"),
        );
        fields.insert("season".into(), "3".into());
        merge(&mut draft, "item:1", &fields);
        fields.insert("review_snapshot".into(), "newer".into());
        merge(&mut draft, "item:1", &fields);
        assert_eq!(draft.groups["item:1"]["metadata_title"], "Unsaved title");
        assert_eq!(draft.groups["item:1"]["review_snapshot"], "original");
        assert!(!draft.groups["item:1"]
            .keys()
            .any(|k| k.starts_with("baseline_")));
    }
}
