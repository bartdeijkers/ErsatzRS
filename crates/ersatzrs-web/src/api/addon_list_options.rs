//! Native option editing and bulk review. Every write keeps the original CAS fence.
use axum::{
    extract::{Form, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use ersatzrs_app::addon_media_list_options::{
    AddonListOptionEdit, AddonListOptionTarget, AddonListOptionUndo,
};
use leptos::prelude::*;
use serde::Deserialize;

use crate::{
    admin::{
        i18n::{load_language_initial_for_state, tr, TextKey},
        pages::media::addon_list_options::{
            editor_view, preview_view, response_document, saved_view,
        },
        server_fns::addon_list_options::{
            load_option_editor, parse_option_items, MAX_OPTION_FORM_BYTES,
        },
    },
    host::state::AppState,
};

#[derive(Deserialize)]
pub struct OptionScopeForm {
    list_id: i64,
    scope: String,
}

#[derive(Deserialize)]
struct ResolvedOptionScope {
    items: String,
    #[serde(default)]
    off_page_count: usize,
}

#[derive(Deserialize)]
pub struct OptionCommandForm {
    command: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    action: String,
    #[serde(default)]
    bulk: bool,
    #[serde(default)]
    off_page_count: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessCheckForm {
    list_id: i64,
    item_id: i64,
    expected_revision: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)] // Native form fields identify three distinct persisted rows.
pub struct AccessCancelForm {
    list_id: i64,
    item_id: i64,
    job_id: i64,
}

fn access_response(state: &AppState, list_id: i64, item_id: i64, success: bool) -> Response {
    let item_url = format!("/media/addon-lists/{list_id}/items/{item_id}#addon-item-access");
    if success {
        return Redirect::to(&item_url).into_response();
    }
    respond(state, list_id, StatusCode::CONFLICT, || {
        view! {
            <p role="alert">{tr(TextKey::AddonAccessError)}</p>
            <a href=item_url>{tr(TextKey::AddonAccessRefresh)}</a>
        }
        .into_any()
    })
}

pub async fn test_access(
    State(state): State<AppState>,
    Form(form): Form<AccessCheckForm>,
) -> Response {
    let success = state
        .application
        .queue_media_list_access_check(form.list_id, form.item_id, form.expected_revision)
        .await
        .is_ok();
    access_response(&state, form.list_id, form.item_id, success)
}

pub async fn cancel_access(
    State(state): State<AppState>,
    Form(form): Form<AccessCancelForm>,
) -> Response {
    let success = state
        .application
        .cancel_media_list_access_check(form.list_id, form.item_id, form.job_id)
        .await
        .is_ok();
    access_response(&state, form.list_id, form.item_id, success)
}

fn decode<T: serde::de::DeserializeOwned>(command: &str) -> Result<T, ()> {
    if command.len() > MAX_OPTION_FORM_BYTES {
        return Err(());
    }
    serde_json::from_str(command).map_err(|_| ())
}

fn respond(
    state: &AppState,
    list_id: i64,
    status: StatusCode,
    body: impl FnOnce() -> AnyView,
) -> Response {
    let language = load_language_initial_for_state(state);
    let owner = Owner::new();
    let html = owner.with(|| {
        provide_context(language);
        response_document(list_id, body())
    });
    (
        status,
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Html(html),
    )
        .into_response()
}

fn failed(state: &AppState, list_id: i64) -> Response {
    respond(state, list_id, StatusCode::CONFLICT, || {
        view! { <p role="alert">{tr(TextKey::AddonOptionsConflict)}</p> }.into_any()
    })
}

pub async fn edit(State(state): State<AppState>, Form(form): Form<OptionScopeForm>) -> Response {
    let Ok(scope) = decode::<ResolvedOptionScope>(&form.scope) else {
        return failed(&state, form.list_id);
    };
    let Ok(ids) = parse_option_items(&scope.items) else {
        return failed(&state, form.list_id);
    };
    // This endpoint is exclusively an explicit bulk selection; empty must not
    // accidentally change list defaults when a filter no longer matches items.
    if ids.is_empty() {
        return failed(&state, form.list_id);
    }
    match load_option_editor(&state, form.list_id, ids).await {
        Ok(Some(mut editor)) => {
            editor.bulk = true;
            editor.off_page_count = scope.off_page_count;
            respond(&state, form.list_id, StatusCode::OK, || editor_view(editor))
        }
        Ok(None) => respond(&state, form.list_id, StatusCode::OK, || {
            view! { <p>{tr(TextKey::AddonOptionsNone)}</p> }.into_any()
        }),
        Err(_) => failed(&state, form.list_id),
    }
}

pub async fn change(
    State(state): State<AppState>,
    Form(form): Form<OptionCommandForm>,
) -> Response {
    let Ok(mut edit) = decode::<AddonListOptionEdit>(&form.command) else {
        return failed(&state, 0);
    };
    let list_id = edit.list_id;
    if form.action == "reset" {
        edit.value = None;
    } else if form.action == "save" {
        let Ok(value) = serde_json::from_str(form.value.as_deref().unwrap_or("false")) else {
            return failed(&state, list_id);
        };
        edit.value = Some(value);
    } else {
        return failed(&state, list_id);
    }
    let Ok(preview) = state
        .application
        .preview_addon_list_option_edit(edit.clone())
        .await
    else {
        return failed(&state, list_id);
    };
    if form.bulk || matches!(&edit.target, AddonListOptionTarget::Items(ids) if ids.len() > 1) {
        let Ok(Some(editor)) = state.application.addon_list_option_editor(list_id).await else {
            return failed(&state, list_id);
        };
        let Some(definition) = editor
            .definition
            .options
            .into_iter()
            .find(|option| option.key == edit.key)
        else {
            return failed(&state, list_id);
        };
        let Ok(items) = state.application.list_addon_media_list_items(list_id).await else {
            return failed(&state, list_id);
        };
        let titles = items
            .into_iter()
            .map(|item| (item.id, item.display_title))
            .collect();
        respond(&state, list_id, StatusCode::OK, || {
            preview_view(preview, &definition, &titles, form.off_page_count)
        })
    } else {
        apply_edit(&state, edit).await
    }
}

async fn apply_edit(state: &AppState, edit: AddonListOptionEdit) -> Response {
    let list_id = edit.list_id;
    match state.application.apply_addon_list_option_edit(edit).await {
        Ok(undo) => respond(state, list_id, StatusCode::OK, || saved_view(&undo)),
        Err(_) => failed(state, list_id),
    }
}

pub async fn apply(State(state): State<AppState>, Form(form): Form<OptionCommandForm>) -> Response {
    let Ok(edit) = decode::<AddonListOptionEdit>(&form.command) else {
        return failed(&state, 0);
    };
    apply_edit(&state, edit).await
}

pub async fn undo(State(state): State<AppState>, Form(form): Form<OptionCommandForm>) -> Response {
    let Ok(undo) = decode::<AddonListOptionUndo>(&form.command) else {
        return failed(&state, 0);
    };
    let list_id = undo.list_id;
    match state.application.undo_addon_list_option_edit(undo).await {
        Ok(_) => respond(&state, list_id, StatusCode::OK, || {
            view! { <p role="status">{tr(TextKey::ChangeUndone)}</p> }.into_any()
        }),
        Err(_) => failed(&state, list_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_forms_require_saved_revision_and_only_owned_identifiers() {
        let form: AccessCheckForm = serde_json::from_value(
            serde_json::json!({"list_id":7,"item_id":12,"expected_revision":19}),
        )
        .unwrap();
        assert_eq!(
            (form.list_id, form.item_id, form.expected_revision),
            (7, 12, 19)
        );
        assert!(serde_json::from_value::<AccessCheckForm>(
            serde_json::json!({"list_id":7,"item_id":12})
        )
        .is_err());
        assert!(serde_json::from_value::<AccessCheckForm>(serde_json::json!({"list_id":7,"item_id":12,"expected_revision":19,"source_url":"https://example.test/override"})).is_err());
        let cancel: AccessCancelForm =
            serde_json::from_value(serde_json::json!({"list_id":7,"item_id":12,"job_id":42}))
                .unwrap();
        assert_eq!((cancel.list_id, cancel.item_id, cancel.job_id), (7, 12, 42));
        assert!(serde_json::from_value::<AccessCancelForm>(
            serde_json::json!({"list_id":7,"item_id":12})
        )
        .is_err());
    }
    #[test]
    fn option_command_form_rejects_oversized_or_snapshot_payloads() {
        assert!(decode::<AddonListOptionEdit>(&" ".repeat(MAX_OPTION_FORM_BYTES + 1)).is_err());
        assert!(decode::<AddonListOptionEdit>(r#"{"draft":{"list_values":{}}}"#).is_err());
    }
}
