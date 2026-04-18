use leptos::prelude::*;

stylance::import_crate_style!(style, "src/ui/primitives/form.module.css");

/// A labelled text input (maps to MudTextField).
#[component]
pub fn TextField(
    label: &'static str,
    value: RwSignal<String>,
    #[prop(default = "text")] input_type: &'static str,
    #[prop(default = "")] placeholder: &'static str,
    #[prop(default = false)] disabled: bool,
    /// Optional validation error message to display below the input.
    #[prop(default = None)]
    error: Option<Signal<Option<String>>>,
) -> impl IntoView {
    view! {
        <div class=style::field>
            <label class=style::label>{label}</label>
            <input
                type=input_type
                placeholder=placeholder
                disabled=disabled
                class=style::input
                prop:value=move || value.get()
                on:input=move |e| value.set(event_target_value(&e))
            />
            {move || error.and_then(|s| s.get()).map(|msg| view! {
                <span class=style::error>{msg}</span>
            })}
        </div>
    }
}

/// A labelled number input.
#[component]
pub fn NumberField(
    label: &'static str,
    value: RwSignal<i64>,
    #[prop(default = None)] min: Option<i64>,
    #[prop(default = None)] max: Option<i64>,
    #[prop(default = false)] disabled: bool,
    #[prop(default = None)] error: Option<Signal<Option<String>>>,
) -> impl IntoView {
    view! {
        <div class=style::field>
            <label class=style::label>{label}</label>
            <input
                type="number"
                disabled=disabled
                min=min.map(|v| v.to_string())
                max=max.map(|v| v.to_string())
                class=style::input
                prop:value=move || value.get().to_string()
                on:input=move |e| {
                    if let Ok(n) = event_target_value(&e).parse::<i64>() {
                        value.set(n);
                    }
                }
            />
            {move || error.and_then(|s| s.get()).map(|msg| view! {
                <span class=style::error>{msg}</span>
            })}
        </div>
    }
}

/// A labelled `<select>` (maps to MudSelect).
/// Pass items as `(value, label)` pairs.
#[component]
pub fn SelectField<T>(
    label: &'static str,
    value: RwSignal<T>,
    items: Vec<(T, String)>,
    #[prop(default = false)] disabled: bool,
    #[prop(default = None)] error: Option<Signal<Option<String>>>,
) -> impl IntoView
where
    T: Clone + PartialEq + std::fmt::Display + std::str::FromStr + Send + Sync + 'static,
{
    let items_view: Vec<_> = items
        .into_iter()
        .map(|(v, lbl)| {
            let v_str = v.to_string();
            view! { <option value=v_str>{lbl}</option> }
        })
        .collect();

    let cls = format!("{} {}", style::input, style::select);

    view! {
        <div class=style::field>
            <label class=style::label>{label}</label>
            <select
                disabled=disabled
                class=cls
                prop:value=move || value.get().to_string()
                on:change=move |e| {
                    if let Ok(parsed) = event_target_value(&e).parse::<T>() {
                        value.set(parsed);
                    }
                }
            >
                {items_view}
            </select>
            {move || error.and_then(|s| s.get()).map(|msg| view! {
                <span class=style::error>{msg}</span>
            })}
        </div>
    }
}

/// A labelled checkbox (maps to MudCheckBox).
#[component]
pub fn CheckBox(
    label: &'static str,
    value: RwSignal<bool>,
    #[prop(default = false)] disabled: bool,
) -> impl IntoView {
    view! {
        <label class=style::checkbox_label>
            <input
                type="checkbox"
                disabled=disabled
                class=style::checkbox
                prop:checked=move || value.get()
                on:change=move |e| {
                    let checked = event_target_checked(&e);
                    value.set(checked);
                }
            />
            <span class=style::checkbox_text>{label}</span>
        </label>
    }
}

/// A labelled textarea.
#[component]
pub fn TextArea(
    label: &'static str,
    value: RwSignal<String>,
    #[prop(default = 4u32)] rows: u32,
    #[prop(default = "")] placeholder: &'static str,
    #[prop(default = false)] disabled: bool,
    #[prop(default = None)] error: Option<Signal<Option<String>>>,
) -> impl IntoView {
    let cls = format!("{} {}", style::input, style::textarea);
    view! {
        <div class=style::field>
            <label class=style::label>{label}</label>
            <textarea
                rows=rows
                placeholder=placeholder
                disabled=disabled
                class=cls
                prop:value=move || value.get()
                on:input=move |e| value.set(event_target_value(&e))
            />
            {move || error.and_then(|s| s.get()).map(|msg| view! {
                <span class=style::error>{msg}</span>
            })}
        </div>
    }
}
