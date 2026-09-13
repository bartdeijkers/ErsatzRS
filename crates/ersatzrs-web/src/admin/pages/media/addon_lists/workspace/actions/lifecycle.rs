//! Explicit source and recovery operations preserve the browser-owned task envelope.
use super::{
    field, item_id, parse, AddonMediaListImportAction, AppState, Result, Submission, TextKey,
};
use ersatzrs_app::commands::{
    ChangeAddonMediaListAddon, ConfirmAddonMediaListRemoval, ControlAddonMediaListImport,
    ResetAddonMediaList,
};

#[expect(
    clippy::too_many_lines,
    reason = "Each explicit lifecycle action preserves the same task envelope and confirmation"
)]
pub(super) async fn execute(
    state: &AppState,
    submission: &mut Submission,
    group: &str,
    intent: &str,
) -> Result<()> {
    let d = &mut submission.draft;
    let fields = d.groups.get(group).cloned().unwrap_or_default();
    match intent {
        "reset-preview" | "change-addon-preview" => {
            d.confirmation = Some(intent.into());
        }
        "confirm-operation" => {
            let current = state
                .application
                .get_addon_media_list(d.list_id)
                .await
                .map_err(|_| TextKey::AddonWorkspaceRetained)?
                .ok_or(TextKey::AddonMediaListNotFound)?;
            if current.draft_revision != d.revision {
                return Err(TextKey::AddonWorkspaceRetained);
            }
            if field(&fields, "acknowledged") != "true" {
                return Err(TextKey::Confirm);
            }
            match d.confirmation.as_deref().ok_or(TextKey::Confirm)? {
                "reset-preview" => {
                    state
                        .application
                        .reset_addon_media_list_editor(
                            ResetAddonMediaList {
                                id: d.list_id,
                                confirmed_name: field(&fields, "confirmed_name").into(),
                            },
                            d.revision,
                        )
                        .await
                        .map_err(|_| TextKey::ResetAndReload)?;
                }
                "change-addon-preview" => {
                    state
                        .application
                        .change_addon_media_list_editor_addon(
                            ChangeAddonMediaListAddon {
                                id: d.list_id,
                                addon_id: field(&fields, "addon_id").into(),
                            },
                            d.revision,
                        )
                        .await
                        .map_err(|_| TextKey::ChangeAddon)?;
                }
                _ => return Err(TextKey::Confirm),
            }
            submission.notice = Some(match d.confirmation.as_deref() {
                Some("reset-preview") => TextKey::ResetQueued,
                _ => TextKey::Saved,
            });
            d.confirmation = None;
        }
        "replacement-search" => {
            state
                .application
                .get_addon_media_list_item(d.list_id, item_id(group)?)
                .await
                .map_err(|_| TextKey::AddonMediaListNotFound)?
                .ok_or(TextKey::AddonMediaListNotFound)?;
            submission.replacement_searched = true;
            submission.replacement_results = state
                .application
                .search_addon_media_list_replacements(field(&fields, "replacement_query"))
                .await
                .map_err(|_| TextKey::InvalidReplacement)?
                .into_iter()
                .map(|r| {
                    crate::admin::server_fns::addon_media_lists::AddonMediaListReplacementRow {
                        media_item_id: r.media_item_id,
                        media_version_id: r.media_version_id,
                        title: r.title,
                        path: r.path,
                    }
                })
                .collect();
        }
        "replacement-clear" => {
            let target = d.groups.entry(group.into()).or_default();
            target.insert("replacement".into(), String::new());
            target.insert("replacement_title".into(), String::new());
        }
        _ if intent.starts_with("replacement:") => {
            let value = &intent[12..];
            let (media, version) = value.split_once(':').ok_or(TextKey::InvalidReplacement)?;
            if media.parse::<i64>().ok().is_none_or(|id| id <= 0)
                || version.parse::<i64>().ok().is_none_or(|id| id <= 0)
            {
                return Err(TextKey::InvalidReplacement);
            }
            let candidate = state
                .application
                .search_addon_media_list_replacements(field(&fields, "replacement_query"))
                .await
                .map_err(|_| TextKey::InvalidReplacement)?
                .into_iter()
                .find(|row| {
                    row.media_item_id.to_string() == media
                        && row.media_version_id.to_string() == version
                })
                .ok_or(TextKey::InvalidReplacement)?;
            let target = d.groups.entry(group.into()).or_default();
            target.insert("replacement".into(), value.into());
            target.insert("replacement_title".into(), candidate.title);
        }
        "access-test" => {
            state
                .application
                .queue_media_list_access_check(
                    d.list_id,
                    parse(&fields, "access_item_id")?,
                    d.revision,
                )
                .await
                .map_err(|_| TextKey::AddonAccessError)?;
        }
        "access-cancel" => {
            state
                .application
                .cancel_media_list_access_check(
                    d.list_id,
                    parse(&fields, "access_item_id")?,
                    parse(&fields, "access_job_id")?,
                )
                .await
                .map_err(|_| TextKey::AddonAccessError)?;
        }
        _ if intent.starts_with("import:") => {
            let action = match &intent[7..] {
                "pause" => AddonMediaListImportAction::Pause,
                "resume" => AddonMediaListImportAction::Resume,
                "retry" => AddonMediaListImportAction::Retry,
                "cancel" => AddonMediaListImportAction::Cancel,
                _ => return Err(TextKey::Import),
            };
            state
                .application
                .control_addon_media_list_import(ControlAddonMediaListImport {
                    id: d.list_id,
                    action,
                })
                .await
                .map_err(|_| TextKey::Import)?;
        }
        _ if intent.starts_with("remove:") => {
            let (id, generation) = intent[7..]
                .split_once(':')
                .ok_or(TextKey::PendingRemovals)?;
            state
                .application
                .confirm_addon_media_list_removal(ConfirmAddonMediaListRemoval {
                    list_id: d.list_id,
                    item_id: id.parse().map_err(|_| TextKey::PendingRemovals)?,
                    expected_generation: generation
                        .parse()
                        .map_err(|_| TextKey::PendingRemovals)?,
                })
                .await
                .map_err(|_| TextKey::PendingRemovals)?;
        }
        _ => return Err(TextKey::AddonWorkspaceRetained),
    }
    Ok(())
}
