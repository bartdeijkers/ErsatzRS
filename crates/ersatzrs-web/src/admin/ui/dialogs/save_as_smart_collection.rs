use leptos::prelude::*;

use crate::admin::i18n::{media_browser_error_message, save_failed, tr, TextKey};
use crate::admin::server_fns::media_browser::{
    save_smart_collection, CollectionForm, CollectionOption,
};
use crate::admin::ui::primitives::dialog::Modal;
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(
    style,
    "src/admin/ui/dialogs/save_as_smart_collection.module.css"
);

const NEW_SMART_COLLECTION_ID: i64 = 0;

#[component]
#[allow(clippy::too_many_lines)]
pub fn SaveAsSmartCollectionDialog(
    open: RwSignal<bool>,
    query: Signal<String>,
    #[prop(default = None)] on_saved: Option<Callback<i64>>,
) -> impl IntoView {
    let refresh: RwSignal<u32> = RwSignal::new(0);
    let options: RwSignal<Vec<CollectionOption>> = RwSignal::new(Vec::new());
    let selected_id: RwSignal<i64> = RwSignal::new(NEW_SMART_COLLECTION_ID);
    let name: RwSignal<String> = RwSignal::new(String::new());
    let name_error: RwSignal<Option<String>> = RwSignal::new(None);
    let query_error: RwSignal<Option<String>> = RwSignal::new(None);

    let toast = use_toast();

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        let _ = refresh.get();
        leptos::task::spawn_local(async move {
            let loaded =
                crate::admin::server_fns::media_browser::list_smart_collections_for_select()
                    .await
                    .unwrap_or_default();
            options.set(loaded);
        });
    });

    let save_action = Action::new(move |form: &CollectionForm| {
        let form = form.clone();
        async move { save_smart_collection(form).await }
    });

    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(id) => {
                    toast.push(tr(TextKey::SmartCollectionSaved), ToastSeverity::Success);
                    if let Some(callback) = on_saved.as_ref() {
                        callback.run(id);
                    }
                    open.set(false);
                    refresh.update(|value| *value += 1);
                    selected_id.set(NEW_SMART_COLLECTION_ID);
                    name.set(String::new());
                    name_error.set(None);
                    query_error.set(None);
                }
                Err(error) => {
                    toast.push(
                        save_failed(media_browser_error_message(error)),
                        ToastSeverity::Error,
                    );
                    open.set(true);
                }
            }
        }
    });

    view! {
        <Modal
            open=open
            title=tr(TextKey::SaveAsSmartCollection).to_string()
            on_close=Callback::new(move |confirmed: bool| {
                if !confirmed {
                    name_error.set(None);
                    query_error.set(None);
                    return;
                }

                let current_query = query.get_untracked().trim().to_string();
                if current_query.is_empty() {
                    query_error.set(Some(tr(TextKey::SearchQueryRequired).to_string()));
                    open.set(true);
                    return;
                }

                let current_id = selected_id.get_untracked();
                let selected_name = if current_id == NEW_SMART_COLLECTION_ID {
                    name.get_untracked().trim().to_string()
                } else {
                    options
                        .with_untracked(|items| smart_collection_name(items, current_id))
                        .unwrap_or_default()
                };

                if selected_name.is_empty() {
                    name_error.set(Some(tr(TextKey::NameRequired).to_string()));
                    open.set(true);
                    return;
                }

                name_error.set(None);
                query_error.set(None);
                save_action.dispatch(CollectionForm {
                    id: current_id,
                    name: selected_name,
                    query: current_query,
                });
            })
            confirm_label=Some(tr(TextKey::Save))
            cancel_label=Some(tr(TextKey::Cancel))
        >
            <div class=style::form>
                <div class=style::field>
                    <label class=style::label>{tr(TextKey::SmartCollection)}</label>
                    <select
                        class=format!("{} {}", style::input, style::select)
                        prop:value=move || selected_id.get().to_string()
                        on:change=move |event| {
                            if let Ok(id) = event_target_value(&event).parse::<i64>() {
                                selected_id.set(id);
                                name_error.set(None);
                                if id == NEW_SMART_COLLECTION_ID {
                                    name.set(String::new());
                                } else if let Some(existing_name) =
                                    options.with_untracked(|items| smart_collection_name(items, id))
                                {
                                    name.set(existing_name);
                                }
                            }
                        }
                    >
                        <option value=NEW_SMART_COLLECTION_ID.to_string()>{tr(TextKey::NewCollection)}</option>
                        {move || {
                            options.get().into_iter().map(|option| view! {
                                <option value=option.id.to_string()>{option.name}</option>
                            }).collect_view()
                        }}
                    </select>
                </div>

                <div class=style::field>
                    <label class=style::label>{tr(TextKey::Name)}</label>
                    <input
                        type="text"
                        class=style::input
                        prop:value=move || name.get()
                        disabled=move || selected_id.get() != NEW_SMART_COLLECTION_ID
                        on:input=move |event| {
                            name.set(event_target_value(&event));
                            name_error.set(None);
                        }
                    />
                    {move || name_error.get().map(|message| view! {
                        <span class=style::error>{message}</span>
                    })}
                </div>

                {move || query_error.get().map(|message| view! {
                    <span class=style::error>{message}</span>
                })}
            </div>
        </Modal>
    }
}

fn smart_collection_name(options: &[CollectionOption], id: i64) -> Option<String> {
    options
        .iter()
        .find(|option| option.id == id)
        .map(|option| option.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_collection_name_finds_existing_option() {
        let options = vec![
            CollectionOption {
                id: 10,
                name: "Alpha".to_string(),
            },
            CollectionOption {
                id: 20,
                name: "Beta".to_string(),
            },
        ];

        assert_eq!(smart_collection_name(&options, 20).as_deref(), Some("Beta"));
        assert_eq!(smart_collection_name(&options, 99), None);
    }
}
