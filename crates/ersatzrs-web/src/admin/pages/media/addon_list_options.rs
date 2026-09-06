//! Schema-rendered native forms, outside the existing metadata hydration islands.
use leptos::prelude::*;

#[cfg(feature = "ssr")]
use crate::admin::i18n::{tr, use_i18n, TextKey};

stylance::import_crate_style!(style, "src/admin/pages/media/addon_list_options.module.css");

pub fn options_available(list_id: i64) -> bool {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<crate::host::state::AppState>();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(state.application.addon_list_option_editor(list_id))
        })
        .is_ok_and(|editor| editor.is_some())
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = list_id;
        false
    }
}

#[component]
pub fn AddonListOptions(list_id: i64, item_ids: Vec<i64>) -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<crate::host::state::AppState>();
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                crate::admin::server_fns::addon_list_options::load_option_editor(
                    &state, list_id, item_ids,
                ),
            )
        });
        match result {
            Ok(Some(editor)) => editor_view(editor),
            Ok(None) => ().into_any(),
            Err(()) => view! { <p role="alert">{tr(TextKey::AddonOptionsConflict)}</p> }.into_any(),
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (list_id, item_ids);
        ().into_any()
    }
}

#[cfg(feature = "ssr")]
fn localized(values: &std::collections::BTreeMap<String, String>) -> String {
    let language = use_i18n().code();
    let lookup = |wanted: &str| {
        values
            .iter()
            .find(|(culture, _)| culture.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| value)
    };
    lookup(language)
        .or_else(|| lookup(language.split('-').next().unwrap_or(language)))
        .or_else(|| lookup("en-US"))
        .or_else(|| lookup("en"))
        .cloned()
        .unwrap_or_default()
}

#[cfg(feature = "ssr")]
fn value_label(
    value: &ersatzrs_domain::AddonMediaListOptionValue,
    definition: &ersatzrs_addon_contract::AddonMediaListOptionDefinition,
) -> String {
    use ersatzrs_domain::AddonMediaListOptionValue;
    match value {
        AddonMediaListOptionValue::Boolean(value) => {
            tr(if *value { TextKey::Yes } else { TextKey::No }).to_owned()
        }
        AddonMediaListOptionValue::Enum(value) => definition
            .choices
            .iter()
            .find(|choice| choice.value == *value)
            .map_or_else(|| value.clone(), |choice| localized(&choice.label)),
    }
}

#[cfg(feature = "ssr")]
pub fn editor_view(editor: crate::admin::server_fns::addon_list_options::OptionEditor) -> AnyView {
    use ersatzrs_addon_contract::AddonMediaListOptionKind;
    use ersatzrs_app::addon_media_list_options::AddonListOptionTarget;
    let list_id = editor.list_id;
    let titles = editor.titles;
    let bulk = editor.bulk;
    let off_page_count = editor.off_page_count;
    let access = if bulk {
        None
    } else {
        editor.access.map(|access| {
            access_view(
                list_id,
                access.item_id,
                access.saved_revision,
                editor.provisional,
                access.status,
            )
        })
    };
    view! {
        <section class=style::options data-testid="addon-list-options" aria-label=tr(TextKey::AddonOptions)>
            <h2>{tr(TextKey::AddonOptions)}</h2>
            <p>{tr(TextKey::AddonOptionsDraftHelp)}</p>
            <p hidden=off_page_count.eq(&0)>{format!("{off_page_count} {}", tr(TextKey::OtherPageItemsAffected))}</p>
            <p role="status" hidden=!editor.provisional>{tr(TextKey::AddonOptionsProvisional)}</p>
            {editor.fields.into_iter().map(|field| {
                let is_default = matches!(field.edit.target, AddonListOptionTarget::ListDefault);
                let disabled = !field.rows.iter().any(|row| row.applicable);
                let current = field.rows.first().map(|row| row.effective_before.clone());
                let mixed = field.rows.iter().any(|row| Some(&row.effective_before) != current.as_ref());
                let label = localized(&field.definition.label);
                let description = localized(&field.definition.description);
                let command = serde_json::to_string(&field.edit).expect("option command serializes");
                let control_id = format!("addon-option-{list_id}-{}", field.definition.key);
                let reason_id = format!("{control_id}-reason");
                let selected = current.as_ref().map(|value| serde_json::to_string(value).expect("value serializes"));
                let control = match field.definition.kind {
                    AddonMediaListOptionKind::Boolean => view! {
                        <input type="checkbox" id=control_id.clone() name="value" value="true" checked=selected.as_deref().is_some_and(|value| value == "true") disabled=disabled aria-describedby=reason_id.clone()/>
                    }.into_any(),
                    AddonMediaListOptionKind::Enum => view! {
                        <select id=control_id.clone() name="value" disabled=disabled aria-describedby=reason_id.clone() required>
                            <option value="" disabled selected=mixed>{tr(TextKey::AddonOptionsMixed)}</option>
                            {field.definition.choices.iter().map(|choice| {
                                let value = serde_json::to_string(&choice.value).expect("choice serializes");
                                let is_selected = !mixed && selected.as_ref() == Some(&value);
                                view! { <option value=value selected=is_selected>{localized(&choice.label)}</option> }
                            }).collect_view()}
                        </select>
                    }.into_any(),
                };
                let summary = field.rows.iter().map(|row| {
                    let who = row.item_id.map_or_else(|| tr(TextKey::AddonOptionsListDefault).to_owned(), |id| format!("{} ({id})", titles.get(&id).cloned().unwrap_or_else(|| tr(TextKey::Item).to_owned())));
                    let inherited = tr(if row.before.is_none() { TextKey::AddonOptionsInherited } else { TextKey::AddonOptionsOverride });
                    let value = value_label(&row.effective_before, &field.definition);
                    view! { <li>{format!("{who}: {value} · {inherited}")}<span hidden=row.applicable>{format!(" · {}", tr(TextKey::AddonOptionsFiniteOnly))}</span></li> }
                }).collect_view();
                view! {
                    <form method="post" action="/api/addon-lists/options/change" class=style::field data-option-key=field.definition.key>
                        <input type="hidden" name="command" value=command/>
                        <input type="hidden" name="bulk" value=bulk.to_string()/>
                        <input type="hidden" name="off_page_count" value=off_page_count.to_string()/>
                        <label for=control_id.clone()>{label}</label>
                        <p>{description}</p>
                        <ul class=style::values>{summary}</ul>
                        <p id=reason_id.clone() hidden=!disabled>{tr(TextKey::AddonOptionsFiniteOnly)}</p>
                        <div class=style::actions>
                            {control}
                            <span hidden=!mixed>{tr(TextKey::AddonOptionsMixed)}</span>
                            <button type="submit" name="action" value="save" disabled=disabled>{tr(if bulk { TextKey::Preview } else { TextKey::SaveChanges })}</button>
                            <button type="submit" name="action" value="reset" formnovalidate>{tr(if is_default { TextKey::AddonOptionsResetPackage } else { TextKey::AddonOptionsResetList })}</button>
                        </div>
                    </form>
                }
            }).collect_view()}
            {access}
        </section>
    }.into_any()
}

#[cfg(feature = "ssr")]
fn access_state_key(state: ersatzrs_app::ports::AddonItemAccessState) -> TextKey {
    use ersatzrs_app::ports::AddonItemAccessState as State;
    match state {
        State::Queued => TextKey::AddonAccessQueued,
        State::Running => TextKey::AddonAccessRunning,
        State::Waiting => TextKey::AddonAccessWaiting,
        State::Complete => TextKey::Completed,
        State::Failed => TextKey::Failed,
        State::Cancelled => TextKey::AddonAccessCancelled,
        State::Stale => TextKey::AddonAccessStale,
    }
}

#[cfg(feature = "ssr")]
fn access_outcome_key(outcome: ersatzrs_addon_contract::MediaListAccessCheckOutcome) -> TextKey {
    use ersatzrs_addon_contract::MediaListAccessCheckOutcome as Outcome;
    match outcome {
        Outcome::Accessible => TextKey::AddonAccessAccessible,
        Outcome::ProfileUnavailable => TextKey::AddonAccessProfileUnavailable,
        Outcome::ProfileLocked => TextKey::AddonAccessProfileLocked,
        Outcome::DecryptionFailed => TextKey::AddonAccessDecryptionFailed,
        Outcome::ProviderRestricted => TextKey::AddonAccessRestricted,
        Outcome::Failed => TextKey::Failed,
    }
}

#[cfg(feature = "ssr")]
fn access_view(
    list_id: i64,
    item_id: i64,
    saved_revision: i32,
    provisional: bool,
    result: Result<Option<ersatzrs_app::ports::AddonItemAccessRead>, ()>,
) -> AnyView {
    use ersatzrs_app::ports::AddonItemAccessState;
    let failed = result.is_err();
    let status = result.ok().flatten();
    let active = status.as_ref().is_some_and(|status| {
        matches!(
            status.state,
            AddonItemAccessState::Queued
                | AddonItemAccessState::Running
                | AddonItemAccessState::Waiting
        )
    });
    let state = status
        .as_ref()
        .map_or(TextKey::AddonAccessNotChecked, |status| {
            access_state_key(status.state)
        });
    let outcome = status
        .as_ref()
        .and_then(|status| status.outcome)
        .map(access_outcome_key);
    let job_id = status.as_ref().map_or(0, |status| status.job_id);
    let retry = status
        .as_ref()
        .and_then(|status| status.next_eligible_at)
        .map(|time| time.to_rfc3339());
    let expires = status
        .as_ref()
        .and_then(|status| status.expires_at)
        .map(|time| time.to_rfc3339());
    view! {
        <section id="addon-item-access" class=style::field data-testid="addon-item-access">
            <h3>{tr(TextKey::AddonAccessTest)}</h3>
            <p>{tr(TextKey::AddonAccessHelp)}</p>
            <p id="addon-access-save-reason" hidden=!provisional>{tr(TextKey::AddonAccessSaveFirst)}</p>
            <p role="alert" hidden=!failed>{tr(TextKey::AddonAccessError)}</p>
            <p role="status">{tr(state)}{outcome.map(|key| view! { <span>" · "{tr(key)}</span> })}</p>
            {retry.map(|time| {
                let datetime = time.clone();
                view! { <p>{tr(TextKey::AddonAccessNextAttempt)}" "<time datetime=datetime>{time}</time></p> }
            })}
            {expires.map(|time| {
                let datetime = time.clone();
                view! { <p>{tr(TextKey::AddonAccessExpires)}" "<time datetime=datetime>{time}</time></p> }
            })}
            <div class=style::actions>
                <form method="post" action="/api/addon-lists/options/test-access">
                    <input type="hidden" name="list_id" value=list_id.to_string()/>
                    <input type="hidden" name="item_id" value=item_id.to_string()/>
                    <input type="hidden" name="expected_revision" value=saved_revision.to_string()/>
                    <button type="submit" disabled={provisional || active} aria-describedby="addon-access-save-reason">{tr(TextKey::AddonAccessTest)}</button>
                </form>
                <a href=format!("/media/addon-lists/{list_id}/items/{item_id}#addon-item-access")>{tr(TextKey::AddonAccessRefresh)}</a>
                <form method="post" action="/api/addon-lists/options/cancel-access" hidden=!active>
                    <input type="hidden" name="list_id" value=list_id.to_string()/>
                    <input type="hidden" name="item_id" value=item_id.to_string()/>
                    <input type="hidden" name="job_id" value=job_id.to_string()/>
                    <button type="submit" disabled=!active>{tr(TextKey::Cancel)}</button>
                </form>
            </div>
        </section>
    }.into_any()
}

#[cfg(feature = "ssr")]
pub fn preview_view(
    preview: ersatzrs_app::addon_media_list_options::AddonListOptionPreview,
    definition: &ersatzrs_addon_contract::AddonMediaListOptionDefinition,
    titles: &std::collections::BTreeMap<i64, String>,
    off_page_count: usize,
) -> AnyView {
    let command = serde_json::to_string(&preview.edit).expect("option command serializes");
    let disabled = !preview.rows.iter().any(|row| row.applicable);
    view! {
        <section class=style::options data-testid="addon-options-preview">
            <h1>{tr(TextKey::Preview)}" · "{localized(&definition.label)}</h1>
            <p>{tr(TextKey::AddonOptionsPreviewHelp)}</p>
            <p>{format!("{} · {} {}", crate::admin::i18n::selected_count(preview.rows.len()), off_page_count.min(preview.rows.len()), tr(TextKey::OtherPageItemsAffected))}</p>
            <div class=style::table_scroll><table>
                <thead><tr><th scope="col">{tr(TextKey::Item)}</th><th scope="col">{tr(TextKey::AddonOptionsBefore)}</th><th scope="col">{tr(TextKey::AddonOptionsAfter)}</th><th scope="col">{tr(TextKey::Status)}</th></tr></thead>
                <tbody>{preview.rows.into_iter().map(|row| {
                    let before = value_label(&row.effective_before, definition);
                    let after = value_label(&row.effective_after, definition);
                    view! { <tr><th scope="row">{row.item_id.map_or_else(|| tr(TextKey::AddonOptionsListDefault).to_owned(), |id| format!("{} ({id})", titles.get(&id).cloned().unwrap_or_else(|| tr(TextKey::Item).to_owned())))}</th><td>{before}</td><td>{after}</td><td>{tr(if !row.applicable { TextKey::AddonOptionsFiniteOnly } else if row.after.is_none() { TextKey::AddonOptionsInherited } else { TextKey::AddonOptionsOverride })}</td></tr> }
                }).collect_view()}</tbody>
            </table></div>
            <form method="post" action="/api/addon-lists/options/apply">
                <input type="hidden" name="command" value=command/>
                <button type="submit" disabled=disabled>{tr(TextKey::Apply)}</button>
            </form>
        </section>
    }.into_any()
}

#[cfg(feature = "ssr")]
pub fn saved_view(undo: &ersatzrs_app::addon_media_list_options::AddonListOptionUndo) -> AnyView {
    view! {
        <section class=style::options data-testid="addon-options-saved">
            <h1>{tr(TextKey::Saved)}</h1>
            <p role="status">{tr(TextKey::AddonOptionsDraftHelp)}</p>
            <form method="post" action="/api/addon-lists/options/undo">
                <input type="hidden" name="command" value=serde_json::to_string(undo).expect("undo serializes")/>
                <button type="submit">{tr(TextKey::UndoLastChange)}</button>
            </form>
        </section>
    }.into_any()
}

#[cfg(feature = "ssr")]
pub fn response_document(list_id: i64, body: AnyView) -> String {
    view! {
        <!DOCTYPE html>
        <html lang=use_i18n().code()><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/><title>{tr(TextKey::AddonOptions)}</title><link rel="stylesheet" href="/public/bundle.css"/></head>
            <body><main class=style::response><a href=format!("/media/addon-lists/{list_id}")>{tr(TextKey::Back)}</a>{body}</main></body>
        </html>
    }.to_html()
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::admin::server_fns::addon_list_options::{OptionEditor, OptionField};
    use ersatzrs_addon_contract::{
        AddonMediaListOptionApplicability, AddonMediaListOptionDefinition,
        AddonMediaListOptionKind, AddonMediaListOptionOperation, AddonMediaListOptionScope,
        AddonMediaListOptionValue as WireValue,
    };
    use ersatzrs_app::addon_media_list_options::{
        AddonListOptionEdit, AddonListOptionPreviewRow, AddonListOptionTarget,
    };
    use ersatzrs_core::repositories::AddonMediaListOptionSchemaRef;
    use ersatzrs_domain::AddonMediaListOptionValue;

    fn fixture(applicable: bool) -> OptionEditor {
        OptionEditor {
            access: None,
            list_id: 7,
            provisional: true,
            bulk: false,
            off_page_count: 0,
            titles: [(12, "Fixture item".into())].into(),
            fields: vec![OptionField {
                definition: AddonMediaListOptionDefinition {
                    key: "prepare_segments".into(),
                    label: [("en-US".into(), "Remove <segments>".into())].into(),
                    description: [("en-US".into(), "Prepare finite media.".into())].into(),
                    scope: AddonMediaListOptionScope::Both,
                    operation_targets: vec![AddonMediaListOptionOperation::Preparation],
                    applicability: AddonMediaListOptionApplicability::FiniteOnly,
                    kind: AddonMediaListOptionKind::Boolean,
                    default: WireValue::Boolean(true),
                    choices: vec![],
                    requires_preparation: true,
                },
                edit: AddonListOptionEdit {
                    list_id: 7,
                    expected_revision: 19,
                    schema: AddonMediaListOptionSchemaRef {
                        addon_id: "fixture".into(),
                        capability_id: "media_list".into(),
                        schema_digest: "a".repeat(64),
                    },
                    key: "prepare_segments".into(),
                    target: AddonListOptionTarget::Items(vec![12]),
                    value: None,
                },
                rows: vec![AddonListOptionPreviewRow {
                    item_id: Some(12),
                    before: None,
                    after: None,
                    effective_before: AddonMediaListOptionValue::Boolean(true),
                    effective_after: AddonMediaListOptionValue::Boolean(true),
                    applicable,
                }],
            }],
        }
    }

    #[test]
    fn options_native_boolean_is_checked_inherited_and_escaped() {
        let owner = Owner::new();
        let html = owner.with(|| editor_view(fixture(true)).to_html());
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("checked"));
        assert!(html.contains("Inherited"));
        assert!(html.contains("Remove &lt;segments&gt;"));
        assert!(html.contains("/api/addon-lists/options/change"));
        assert!(html.contains("provisional"));
        assert!(!html.contains("definition_json"));
        assert!(!html.contains("item_values"));
    }

    #[test]
    fn options_live_control_is_disabled_but_sparse_reset_remains_available() {
        let owner = Owner::new();
        let html = owner.with(|| editor_view(fixture(false)).to_html());
        assert!(html.contains("this option requires finite media"));
        let input = html
            .split("type=\"checkbox\"")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(input.contains("disabled"));
        let reset = html
            .split("value=\"reset\"")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(!reset.contains("disabled"));
    }

    #[test]
    fn options_undo_retains_original_revision_and_unbound_schema() {
        use ersatzrs_app::addon_media_list_options::{
            AddonListOptionPreviousValue, AddonListOptionUndo,
        };
        let editor = fixture(true);
        let undo = AddonListOptionUndo {
            list_id: 7,
            expected_revision: 20,
            schema: editor.fields[0].edit.schema.clone(),
            previous_schema: None,
            key: "prepare_segments".into(),
            previous: vec![AddonListOptionPreviousValue {
                item_id: Some(12),
                value: None,
            }],
        };
        let owner = Owner::new();
        let html = owner.with(|| saved_view(&undo).to_html());
        assert!(html.contains("expected_revision&quot;:20"));
        assert!(html.contains("previous_schema&quot;:null"));
        assert!(html.contains("/api/addon-lists/options/undo"));
    }

    #[test]
    fn options_enum_uses_package_culture_and_sparse_inheritance() {
        use ersatzrs_addon_contract::AddonMediaListOptionChoice;
        let mut editor = fixture(true);
        let field = &mut editor.fields[0];
        field.definition.kind = AddonMediaListOptionKind::Enum;
        field.definition.default = WireValue::Enum("automatic".into());
        field.definition.choices = vec![AddonMediaListOptionChoice {
            value: "automatic".into(),
            label: [
                ("en-US".into(), "Automatic".into()),
                ("fr-FR".into(), "Automatique".into()),
            ]
            .into(),
        }];
        field.rows[0].effective_before = AddonMediaListOptionValue::Enum("automatic".into());
        let owner = Owner::new();
        let html = owner.with(|| {
            provide_context(crate::admin::i18n::UiLanguage::French);
            editor_view(editor).to_html()
        });
        assert!(html.contains("Automatique"));
        assert!(html.contains("Héritée"));
        assert!(html.contains("&quot;automatic&quot;"));
        assert!(html.contains("selected"));
    }

    #[test]
    fn options_preview_keeps_exact_command_and_off_page_context() {
        let mut editor = fixture(true);
        let field = editor.fields.remove(0);
        let owner = Owner::new();
        let html = owner.with(|| {
            preview_view(
                ersatzrs_app::addon_media_list_options::AddonListOptionPreview {
                    edit: field.edit,
                    rows: field.rows,
                },
                &field.definition,
                &editor.titles,
                1,
            )
            .to_html()
        });
        assert!(html.contains("Fixture item (12)"));
        assert!(html.contains("expected_revision&quot;:19"));
        assert!(html.contains("Items&quot;:[12]"));
        assert!(html.contains(tr(TextKey::OtherPageItemsAffected)));
        assert!(html.contains("/api/addon-lists/options/apply"));
    }

    #[test]
    fn access_native_provisional_form_requires_saved_options() {
        let owner = Owner::new();
        let html = owner.with(|| access_view(7, 12, 19, true, Ok(None)).to_html());
        assert!(html.contains("/api/addon-lists/options/test-access"));
        assert!(html.contains("name=\"expected_revision\" value=\"19\""));
        assert!(html.contains("Save an option first"));
        let submit = html
            .split("<button")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(submit.contains("disabled"));
        assert!(html.contains("/media/addon-lists/7/items/12#addon-item-access"));
        assert!(!html.contains("source_url"));
        assert!(!html.contains("context_fingerprint"));
    }

    #[test]
    fn access_active_native_form_has_owned_cancel_and_retry_time() {
        use ersatzrs_app::ports::{AddonItemAccessRead, AddonItemAccessState};
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-06T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let status = AddonItemAccessRead {
            job_id: 42,
            state: AddonItemAccessState::Waiting,
            outcome: None,
            next_eligible_at: Some(now),
            expires_at: None,
        };
        let owner = Owner::new();
        let html = owner.with(|| access_view(7, 12, 19, false, Ok(Some(status))).to_html());
        assert!(html.contains("Waiting to retry access check"));
        assert!(html.contains("2026-09-06T12:00:00+00:00"));
        assert!(html.contains("name=\"job_id\" value=\"42\""));
        let cancel_form = html
            .split("action=\"/api/addon-lists/options/cancel-access\"")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(!cancel_form.contains("hidden"));
    }

    #[test]
    fn access_controls_are_absent_for_unsupported_and_bulk_contexts() {
        let owner = Owner::new();
        assert!(!owner
            .with(|| editor_view(fixture(true)).to_html())
            .contains("addon-item-access"));
        let mut editor = fixture(true);
        editor.bulk = true;
        editor.access = Some(
            crate::admin::server_fns::addon_list_options::ItemAccessEditor {
                item_id: 12,
                saved_revision: 19,
                status: Ok(None),
            },
        );
        assert!(!owner
            .with(|| editor_view(editor).to_html())
            .contains("addon-item-access"));
    }

    #[test]
    fn access_closed_outcomes_and_states_have_translations_in_every_culture() {
        use ersatzrs_addon_contract::MediaListAccessCheckOutcome as Outcome;
        use ersatzrs_app::ports::AddonItemAccessState as State;
        for language in crate::admin::languages::LANGUAGES {
            let language = crate::admin::i18n::UiLanguage::from_config(language.code);
            for outcome in [
                Outcome::Accessible,
                Outcome::ProfileUnavailable,
                Outcome::ProfileLocked,
                Outcome::DecryptionFailed,
                Outcome::ProviderRestricted,
                Outcome::Failed,
            ] {
                assert!(
                    !crate::admin::i18n::text(language, access_outcome_key(outcome)).is_empty()
                );
            }
            for state in [
                State::Queued,
                State::Running,
                State::Waiting,
                State::Complete,
                State::Failed,
                State::Cancelled,
                State::Stale,
            ] {
                assert!(!crate::admin::i18n::text(language, access_state_key(state)).is_empty());
            }
        }
    }
}
