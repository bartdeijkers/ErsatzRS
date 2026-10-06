//! Explicit per-attachment overrides shared by the existing schedule editors.

use std::collections::BTreeMap;

use leptos::prelude::*;
use serde_json::Value;

use crate::admin::i18n::{tr, TextKey};

pub(super) fn variables_json(variables: &BTreeMap<i64, String>) -> String {
    let values: BTreeMap<_, _> = variables
        .iter()
        .map(|(id, value)| {
            (
                *id,
                serde_json::from_str::<Value>(value)
                    .unwrap_or_else(|_| Value::String(value.clone())),
            )
        })
        .collect();
    serde_json::to_string(&values).unwrap_or_else(|_| "{}".to_owned())
}

pub(super) fn variables_editor<F>(
    ids: &[i64],
    names: Vec<(i64, String)>,
    variables: &BTreeMap<i64, String>,
    classes: (&'static str, &'static str),
    on_input: F,
) -> AnyView
where
    F: Fn(i64, String) + Copy + 'static,
{
    let names: BTreeMap<_, _> = names.into_iter().collect();
    let fields = ids
        .iter()
        .map(|id| {
            let id = *id;
            let name = names.get(&id).map_or_else(
                || format!("{} {id}", tr(TextKey::GraphicsElement)),
                Clone::clone,
            );
            let value = variables.get(&id).cloned().unwrap_or_default();
            view! {
                <label class=classes.0>
                    <span>{format!("{} — {name}", tr(TextKey::GraphicsItemVariables))}</span>
                    <textarea class=classes.1 rows="4" maxlength="65536" placeholder="{}"
                        prop:value=value
                        data-graphics-item-variables=id.to_string()
                        on:input=move |event| on_input(id, event_target_value(&event)) />
                </label>
            }
        })
        .collect_view();
    view! {
        <div data-graphics-item-variable-editor>
            <p>{tr(TextKey::GraphicsItemVariablesHelp)}</p>
            {fields}
        </div>
    }
    .into_any()
}
