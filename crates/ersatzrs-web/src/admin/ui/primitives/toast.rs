use std::collections::VecDeque;

use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::ui::primitives::icon::{icon_style, IconName};

stylance::import_crate_style!(style, "src/admin/ui/primitives/toast.module.css");

const MAX_VISIBLE_TOASTS: usize = 5;
const ENTER_DURATION_MS: u32 = 1_000;
#[cfg(any(target_arch = "wasm32", test))]
const VISIBLE_DURATION_MS: u32 = 5_000;
const EXIT_DURATION_MS: u32 = 2_000;
#[cfg(target_arch = "wasm32")]
const LIFECYCLE_TICK_MS: u32 = 50;

#[cfg(any(target_arch = "wasm32", test))]
fn elapsed_lifecycle_ms(previous_ms: f64, current_ms: f64) -> u32 {
    let elapsed_ms = (current_ms - previous_ms).max(0.0);
    let elapsed = std::time::Duration::from_secs_f64(elapsed_ms.min(f64::from(u32::MAX)) / 1_000.0);
    u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX)
}

/// Severity level for a toast notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastSeverity {
    Success,
    Info,
    Warning,
    Error,
}

/// Optional navigation offer rendered inside a toast, e.g. "View Collection"
/// after a stay-in-context add flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastAction {
    pub label: String,
    pub href: String,
}

#[allow(dead_code)] // `Visible` is reached by the client-only lifecycle controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToastPhase {
    Entering,
    Visible,
    Exiting,
}

impl ToastPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Entering => "entering",
            Self::Visible => "visible",
            Self::Exiting => "exiting",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub severity: ToastSeverity,
    pub action: Option<ToastAction>,
    phase: ToastPhase,
    remaining_ms: u32,
    paused: bool,
}

impl Toast {
    fn new(id: u64, message: String, severity: ToastSeverity, action: Option<ToastAction>) -> Self {
        Self {
            id,
            message,
            severity,
            action,
            phase: ToastPhase::Entering,
            remaining_ms: ENTER_DURATION_MS,
            paused: false,
        }
    }

    /// Advance this toast and return `true` once its exit state has finished.
    #[cfg(any(target_arch = "wasm32", test))]
    fn advance(&mut self, mut elapsed_ms: u32) -> bool {
        if self.paused {
            return false;
        }

        loop {
            if elapsed_ms < self.remaining_ms {
                self.remaining_ms -= elapsed_ms;
                return false;
            }

            elapsed_ms -= self.remaining_ms;
            match self.phase {
                ToastPhase::Entering => {
                    self.phase = ToastPhase::Visible;
                    self.remaining_ms = VISIBLE_DURATION_MS;
                }
                ToastPhase::Visible => {
                    self.phase = ToastPhase::Exiting;
                    self.remaining_ms = EXIT_DURATION_MS;
                }
                ToastPhase::Exiting => return true,
            }
        }
    }
}

#[derive(Default)]
struct ToastQueue {
    visible: Vec<Toast>,
    queued: VecDeque<Toast>,
    next_id: u64,
    #[cfg(target_arch = "wasm32")]
    last_tick_ms: Option<f64>,
}

impl ToastQueue {
    fn push(
        &mut self,
        message: impl Into<String>,
        severity: ToastSeverity,
        action: Option<ToastAction>,
    ) -> bool {
        let message = message.into().trim().to_owned();
        if message.is_empty() || self.contains_message(&message) {
            return false;
        }

        let toast = Toast::new(self.next_id, message, severity, action);
        self.next_id += 1;
        if self.visible.len() < MAX_VISIBLE_TOASTS {
            self.visible.push(toast);
        } else {
            self.queued.push_back(toast);
        }
        true
    }

    fn contains_message(&self, message: &str) -> bool {
        self.visible
            .iter()
            .chain(&self.queued)
            .any(|toast| toast.message == message)
    }

    fn set_paused(&mut self, id: u64, paused: bool) {
        if let Some(toast) = self.visible.iter_mut().find(|toast| toast.id == id) {
            toast.paused = paused;
        }
    }

    fn dismiss(&mut self, id: u64) {
        if let Some(toast) = self.visible.iter_mut().find(|toast| toast.id == id) {
            toast.phase = ToastPhase::Exiting;
            toast.remaining_ms = EXIT_DURATION_MS;
            // A close click happens while the pointer is over the toast. It
            // must still complete its exit without requiring pointer leave.
            toast.paused = false;
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn advance(&mut self, elapsed_ms: u32) {
        self.visible.retain_mut(|toast| !toast.advance(elapsed_ms));
        self.promote_queued();
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn promote_queued(&mut self) {
        while self.visible.len() < MAX_VISIBLE_TOASTS {
            let Some(toast) = self.queued.pop_front() else {
                break;
            };
            self.visible.push(toast);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn is_empty(&self) -> bool {
        self.visible.is_empty() && self.queued.is_empty()
    }

    #[cfg(target_arch = "wasm32")]
    fn start_clock(&mut self) {
        self.last_tick_ms = Some(js_sys::Date::now());
    }

    #[cfg(target_arch = "wasm32")]
    fn stop_clock(&mut self) {
        self.last_tick_ms = None;
    }

    #[cfg(target_arch = "wasm32")]
    fn advance_to_now(&mut self) {
        let current_ms = js_sys::Date::now();
        let elapsed_ms = self
            .last_tick_ms
            .replace(current_ms)
            .map_or(0, |previous_ms| {
                elapsed_lifecycle_ms(previous_ms, current_ms)
            });
        self.advance(elapsed_ms);
    }
}

/// Process-level toast store. Provide once in the app root and use `push_toast` in islands.
#[derive(Clone, Copy)]
pub struct ToastStore {
    inner: RwSignal<ToastQueue>,
    #[cfg(target_arch = "wasm32")]
    ticker_running: RwSignal<bool>,
}

impl ToastStore {
    pub fn new() -> Self {
        Self {
            inner: RwSignal::new(ToastQueue::default()),
            #[cfg(target_arch = "wasm32")]
            ticker_running: RwSignal::new(false),
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

        let accepted = self
            .inner
            .try_update(|queue| {
                #[cfg(target_arch = "wasm32")]
                queue.advance_to_now();
                queue.push(message, severity, action)
            })
            .unwrap_or(false);
        if accepted {
            #[cfg(target_arch = "wasm32")]
            self.ensure_ticker();
        }
    }

    pub fn remove(&self, id: u64) {
        self.inner.update(|queue| {
            #[cfg(target_arch = "wasm32")]
            queue.advance_to_now();
            queue.dismiss(id);
        });
        #[cfg(target_arch = "wasm32")]
        self.ensure_ticker();
    }

    fn set_paused(&self, id: u64, paused: bool) {
        self.inner.update(|queue| {
            #[cfg(target_arch = "wasm32")]
            queue.advance_to_now();
            queue.set_paused(id, paused);
        });
    }

    pub fn toasts(&self) -> Signal<Vec<Toast>> {
        let inner = self.inner;
        Signal::derive(move || inner.with(|queue| queue.visible.clone()))
    }

    #[cfg(target_arch = "wasm32")]
    fn ensure_ticker(&self) {
        if self.ticker_running.get_untracked() {
            return;
        }

        self.ticker_running.set(true);
        self.inner.update(ToastQueue::start_clock);
        let store = *self;
        leptos::task::spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(LIFECYCLE_TICK_MS).await;
                store.inner.update(ToastQueue::advance_to_now);
                if store.inner.with_untracked(ToastQueue::is_empty) {
                    store.inner.update(ToastQueue::stop_clock);
                    store.ticker_running.set(false);
                    break;
                }
            }
        });
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

fn severity_name(s: ToastSeverity) -> &'static str {
    match s {
        ToastSeverity::Success => "success",
        ToastSeverity::Info => "info",
        ToastSeverity::Warning => "warning",
        ToastSeverity::Error => "error",
    }
}

fn severity_icon(s: ToastSeverity) -> IconName {
    match s {
        ToastSeverity::Success => IconName::AlertSuccess,
        ToastSeverity::Info => IconName::Info,
        ToastSeverity::Warning => IconName::Warning,
        ToastSeverity::Error => IconName::Error,
    }
}

fn phase_class(phase: ToastPhase) -> &'static str {
    match phase {
        ToastPhase::Entering => style::toast_entering,
        ToastPhase::Visible => style::toast_visible,
        ToastPhase::Exiting => style::toast_exiting,
    }
}

/// Renders up to five oldest-first toasts in the top-right corner.
#[component]
pub fn ToastContainer() -> impl IntoView {
    let store = use_toast();
    let toasts = store.toasts();

    view! {
        <div class=style::container data-toast-stack="">
            <For
                each=move || toasts.get()
                key=|t| t.id
                children=move |toast| {
                    let store = store;
                    let id = toast.id;
                    let severity = toast.severity;
                    let state = Memo::new(move |_| {
                        store.inner.with(|queue| {
                            queue
                                .visible
                                .iter()
                                .find(|current| current.id == id)
                                .map_or((ToastPhase::Exiting, false), |current| {
                                    (current.phase, current.paused)
                                })
                        })
                    });
                    let cls = move || {
                        let (phase, paused) = state.get();
                        let mut cls = format!(
                            "{} {} {}",
                            style::toast_base,
                            severity_class(severity),
                            phase_class(phase)
                        );
                        if paused {
                            cls.push(' ');
                            cls.push_str(style::toast_paused);
                        }
                        cls
                    };
                    view! {
                        <div
                            class=cls
                            role="alert"
                            aria-live="polite"
                            aria-atomic="true"
                            data-toast-id=id
                            data-toast-severity=severity_name(severity)
                            data-toast-phase=move || state.get().0.as_str()
                            data-toast-paused=move || state.get().1.to_string()
                            on:pointerenter=move |_| store.set_paused(id, true)
                            on:pointerleave=move |_| store.set_paused(id, false)
                            on:pointerdown=move |_| store.set_paused(id, true)
                            on:pointerup=move |_| store.set_paused(id, false)
                            on:pointercancel=move |_| store.set_paused(id, false)
                        >
                            <span class=style::toast_icon aria-hidden="true">
                                <span style=icon_style(severity_icon(severity), "22px")></span>
                            </span>
                            <span class=style::toast_text>
                                {toast.message.clone()}
                            </span>
                            {toast.action.clone().map(|action| view! {
                                <a class=style::toast_action href=action.href>
                                    {action.label}
                                </a>
                            })}
                            <button
                                type="button"
                                class=style::toast_close
                                aria-label=tr(TextKey::Close)
                                on:click=move |_| store.remove(id)
                            >
                                <span
                                    style=icon_style(IconName::Close, "22px")
                                    aria-hidden="true"
                                ></span>
                            </button>
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

    #[test]
    fn queue_trims_messages_and_rejects_empty_or_duplicate_messages() {
        let mut queue = ToastQueue::default();

        assert!(!queue.push(" \n\t ", ToastSeverity::Info, None));
        assert!(queue.push("  Saved  ", ToastSeverity::Success, None));
        assert_eq!(queue.visible[0].message, "Saved");
        assert!(!queue.push("Saved", ToastSeverity::Error, None));
        assert!(!queue.push(" Saved ", ToastSeverity::Warning, None));
        assert_eq!(queue.visible.len(), 1);
    }

    #[test]
    fn queue_displays_five_oldest_first_and_promotes_overflow_in_order() {
        let mut queue = ToastQueue::default();
        for number in 1..=7 {
            assert!(queue.push(format!("Toast {number}"), ToastSeverity::Info, None));
        }

        assert_eq!(
            queue
                .visible
                .iter()
                .map(|toast| toast.message.as_str())
                .collect::<Vec<_>>(),
            vec!["Toast 1", "Toast 2", "Toast 3", "Toast 4", "Toast 5"]
        );
        assert_eq!(
            queue
                .queued
                .iter()
                .map(|toast| toast.message.as_str())
                .collect::<Vec<_>>(),
            vec!["Toast 6", "Toast 7"]
        );

        queue.dismiss(0);
        queue.advance(EXIT_DURATION_MS);
        assert_eq!(
            queue
                .visible
                .iter()
                .map(|toast| toast.message.as_str())
                .collect::<Vec<_>>(),
            vec!["Toast 2", "Toast 3", "Toast 4", "Toast 5", "Toast 6"]
        );
    }

    #[test]
    fn lifecycle_advances_through_enter_visible_and_exit_states() {
        let mut toast = Toast::new(0, "Saved".to_owned(), ToastSeverity::Success, None);

        assert_eq!(toast.phase, ToastPhase::Entering);
        assert!(!toast.advance(ENTER_DURATION_MS));
        assert_eq!(toast.phase, ToastPhase::Visible);
        assert!(!toast.advance(VISIBLE_DURATION_MS));
        assert_eq!(toast.phase, ToastPhase::Exiting);
        assert!(toast.advance(EXIT_DURATION_MS));
    }

    #[test]
    fn lifecycle_elapsed_time_uses_the_actual_callback_delay() {
        assert_eq!(elapsed_lifecycle_ms(1_000.0, 1_075.9), 75);
        assert_eq!(elapsed_lifecycle_ms(1_075.0, 1_000.0), 0);
        assert_eq!(
            elapsed_lifecycle_ms(0.0, f64::from(u32::MAX) + 1_000.0),
            u32::MAX
        );
    }

    #[test]
    fn interaction_pause_preserves_the_remaining_lifecycle() {
        let mut queue = ToastQueue::default();
        assert!(queue.push("Saved", ToastSeverity::Success, None));
        let id = queue.visible[0].id;
        queue.advance(400);
        let remaining = queue.visible[0].remaining_ms;

        queue.set_paused(id, true);
        queue.advance(10_000);
        assert_eq!(queue.visible[0].phase, ToastPhase::Entering);
        assert_eq!(queue.visible[0].remaining_ms, remaining);

        queue.set_paused(id, false);
        queue.advance(remaining);
        assert_eq!(queue.visible[0].phase, ToastPhase::Visible);
    }

    #[test]
    fn manual_close_uses_exit_state_then_removes_immediately_at_its_end() {
        let mut queue = ToastQueue::default();
        assert!(queue.push("Saved", ToastSeverity::Success, None));
        let id = queue.visible[0].id;
        queue.set_paused(id, true);

        queue.dismiss(id);
        assert_eq!(queue.visible[0].phase, ToastPhase::Exiting);
        assert!(!queue.visible[0].paused);
        queue.advance(EXIT_DURATION_MS - 1);
        assert_eq!(queue.visible.len(), 1);
        queue.advance(1);
        assert!(queue.visible.is_empty());
    }
}
