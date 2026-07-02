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

/// Optional navigation offer rendered inside a toast, e.g. "View Collection"
/// after a stay-in-context add flow.
#[derive(Clone)]
pub struct ToastAction {
    pub label: String,
    pub href: String,
}

#[derive(Clone)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub severity: ToastSeverity,
    pub action: Option<ToastAction>,
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
        self.push_toast(message, severity, None);
    }

    /// Push a toast carrying an explicit navigation action link.
    pub fn push_with_action(
        &self,
        message: impl Into<String>,
        severity: ToastSeverity,
        action: ToastAction,
    ) {
        self.push_toast(message, severity, Some(action));
    }

    fn push_toast(
        &self,
        message: impl Into<String>,
        severity: ToastSeverity,
        action: Option<ToastAction>,
    ) {
        let id = self.next_id.get_untracked();
        self.next_id.update(|n| *n += 1);
        self.inner.update(|v| {
            v.push(Toast {
                id,
                message: message.into(),
                severity,
                action,
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
///
/// Hydrated islands do not share a context tree, so a toast pushed from a
/// page island would otherwise land in a store the shell's `ToastContainer`
/// island never reads. On the client every caller therefore shares one
/// process-wide store, regardless of island hydration order; on the server
/// the store stays request-scoped via context.
pub fn use_toast() -> ToastStore {
    #[cfg(target_arch = "wasm32")]
    {
        thread_local! {
            static GLOBAL_TOAST_STORE: std::cell::Cell<Option<ToastStore>> =
                const { std::cell::Cell::new(None) };
        }
        GLOBAL_TOAST_STORE.with(|cell| {
            if let Some(store) = cell.get() {
                store
            } else {
                let store = ToastStore::new();
                cell.set(Some(store));
                store
            }
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use_context::<ToastStore>().unwrap_or_else(provide_toast_store)
    }
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
                            <span class=style::toast_text>
                                {toast.message.clone()}
                                {toast.action.clone().map(|action| view! {
                                    <a class=style::toast_action href=action.href>
                                        {action.label}
                                    </a>
                                })}
                            </span>
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
