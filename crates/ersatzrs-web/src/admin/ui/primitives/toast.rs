use leptos::prelude::*;

stylance::import_crate_style!(style, "src/admin/ui/primitives/toast.module.css");

/// Severity level for a toast notification.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastSeverity {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Clone)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub severity: ToastSeverity,
}

/// Process-level toast store. Provide once in the app root and use `push_toast` in islands.
#[derive(Clone, Copy)]
pub struct ToastStore {
    inner: RwSignal<Vec<Toast>>,
    next_id: RwSignal<u64>,
}

impl ToastStore {
    pub fn new() -> Self {
        Self {
            inner: RwSignal::new(Vec::new()),
            next_id: RwSignal::new(0),
        }
    }

    pub fn push(&self, message: impl Into<String>, severity: ToastSeverity) {
        let id = self.next_id.get_untracked();
        self.next_id.update(|n| *n += 1);
        self.inner.update(|v| {
            v.push(Toast {
                id,
                message: message.into(),
                severity,
            });
        });
    }

    pub fn remove(&self, id: u64) {
        self.inner.update(|v| v.retain(|t| t.id != id));
    }

    pub fn toasts(&self) -> Signal<Vec<Toast>> {
        self.inner.into()
    }
}

/// Provide `ToastStore` in the Leptos context tree.
pub fn provide_toast_store() -> ToastStore {
    let store = ToastStore::new();
    provide_context(store);
    store
}

/// Access the `ToastStore` from any component.
pub fn use_toast() -> ToastStore {
    expect_context::<ToastStore>()
}

fn severity_class(s: ToastSeverity) -> &'static str {
    match s {
        ToastSeverity::Success => style::toast_success,
        ToastSeverity::Info => style::toast_info,
        ToastSeverity::Warning => style::toast_warning,
        ToastSeverity::Error => style::toast_error,
    }
}

fn severity_icon(s: ToastSeverity) -> &'static str {
    match s {
        ToastSeverity::Success => "✓",
        ToastSeverity::Info => "ℹ",
        ToastSeverity::Warning => "⚠",
        ToastSeverity::Error => "✕",
    }
}

/// Renders all active toasts in the bottom-right corner.
#[component]
pub fn ToastContainer() -> impl IntoView {
    let store = use_toast();
    let toasts = store.toasts();

    view! {
        <div class=style::container>
            <For
                each=move || toasts.get()
                key=|t| t.id
                children=move |toast| {
                    let store = store;
                    let id = toast.id;
                    let cls = format!("{} {}", style::toast_base, severity_class(toast.severity));
                    view! {
                        <div class=cls>
                            <span class=style::toast_icon>
                                {severity_icon(toast.severity)}
                            </span>
                            <span class=style::toast_text>{toast.message.clone()}</span>
                            <button
                                class=style::toast_close
                                on:click=move |_| store.remove(id)
                            >"×"</button>
                        </div>
                    }
                }
            />
        </div>
    }
}
