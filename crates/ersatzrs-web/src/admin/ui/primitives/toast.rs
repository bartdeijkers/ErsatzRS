use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};

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

    /// Queue a toast, but only on the client.
    ///
    /// The toast store is deliberately not hydration-stable: on the server it
    /// is request-scoped context, on the client a single process-wide store
    /// that starts empty (see [`use_toast`]). A toast pushed while SSR is still
    /// rendering therefore serializes a `<For>` child into
    /// [`ToastContainer`]'s HTML that the client store can never reproduce, and
    /// hydration aborts with "expected a marker node" the moment it reaches the
    /// container — taking every island on the page down with it.
    ///
    /// Making the write itself inert off-wasm keeps that whole failure class
    /// unrepresentable, no matter which call site reaches it. Server-side load
    /// failures belong in the stable resource surface
    /// (`admin_result_view` / `AdminResourceState`), which is serialized as
    /// part of the page instead of as shell state; `cargo xtask
    /// hydration-lint`'s rule-3 enforces that split.
    fn push_toast(
        &self,
        message: impl Into<String>,
        severity: ToastSeverity,
        action: Option<ToastAction>,
    ) {
        if !cfg!(target_arch = "wasm32") {
            return;
        }

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

/// Keep an interactive list usable after a refresh failure while surfacing
/// the operational error through the shared, localized toast channel.
///
/// **Client-side channel only.** `ToastStore::push` is inert off-wasm, so on a
/// server-executed loader (`Resource::new`, `Resource::new_blocking`) this
/// silently degrades to `T::default()` and the failure is never reported. Use it
/// from `Action` callbacks, `Effect`s, and `LocalResource` loaders — anything
/// that runs after hydration. A server-backed load that can fail belongs in
/// `admin_result_view`, which renders the failure inside the stable
/// `[data-admin-resource]` wrapper. `cargo xtask hydration-lint` rule-3 fails
/// the build on the wrong side of that split.
pub fn load_or_notify<T, E>(result: Result<T, E>, toast: ToastStore) -> T
where
    T: Default,
    E: std::fmt::Display,
{
    match result {
        Ok(value) => value,
        Err(error) => {
            toast.push(load_failed_message(error), ToastSeverity::Error);
            T::default()
        }
    }
}

/// The localized load-failure message, shared with [`load_or_notify`].
///
/// Server-executed loaders render their failure in a stable, always-present
/// element instead of the toast channel (see [`load_or_notify`]); using the same
/// message keeps the wording identical whichever surface reports it.
pub fn load_failed_message(error: impl std::fmt::Display) -> String {
    format!("{}: {error}", tr(TextKey::ErrorOccurred))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The server must never serialize a toast into `ToastContainer`'s HTML:
    /// the client store starts empty, so the extra `<For>` child is a
    /// guaranteed hydration marker mismatch that aborts every island on the
    /// page. Keeping the write inert off-wasm makes that unrepresentable.
    #[test]
    fn pushing_off_wasm_leaves_the_store_empty() {
        let owner = Owner::new();
        owner.with(|| {
            let store = ToastStore::new();
            store.push("boom", ToastSeverity::Error);
            store.push_with_action(
                "boom",
                ToastSeverity::Error,
                ToastAction {
                    label: "View".to_owned(),
                    href: "/".to_owned(),
                },
            );

            assert!(
                store.toasts().get_untracked().is_empty(),
                "a server-side push must not reach the serialized toast container"
            );
        });
    }

    /// `load_or_notify` still yields a usable default off-wasm, so an island
    /// that reaches it from a server-executed loader renders rather than panics
    /// — it just cannot report the failure, which is what rule-3 guards.
    #[test]
    fn load_or_notify_returns_the_default_without_queueing_a_toast() {
        let owner = Owner::new();
        owner.with(|| {
            let store = ToastStore::new();
            let value = load_or_notify(Err::<Vec<u8>, _>("nope"), store);

            assert!(value.is_empty());
            assert!(store.toasts().get_untracked().is_empty());
        });
    }
}
