//! First-import landing: wait for discovery before showing an editable list.
use leptos::prelude::*;

use super::super::{tr, AddonMediaListImportState, TextKey};

pub(super) fn active(state: AddonMediaListImportState) -> bool {
    matches!(
        state,
        AddonMediaListImportState::Queued
            | AddonMediaListImportState::Discovering
            | AddonMediaListImportState::Verifying
    )
}

/// No editable fields are present on this first-import landing, so replacing it
/// when discovery settles cannot discard operator edits. Paused/failed imports
/// return to the normal workspace and its existing native recovery controls.
#[island]
pub fn ImportLandingIsland(list_id: i64) -> impl IntoView {
    #[cfg(target_arch = "wasm32")]
    {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        on_cleanup({
            let cancelled = Arc::clone(&cancelled);
            move || cancelled.store(true, Ordering::Relaxed)
        });
        leptos::task::spawn_local(async move {
            let mut delay = 1500_u32;
            while !cancelled.load(Ordering::Relaxed) {
                gloo_timers::future::TimeoutFuture::new(delay).await;
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match super::super::get_addon_media_list_form(list_id).await {
                    Ok(Some(form)) => {
                        if cancelled.load(Ordering::Relaxed) {
                            break;
                        }
                        if form
                            .import_progress
                            .as_ref()
                            .is_none_or(|p| !active(p.state))
                        {
                            // Preserve arrival notices such as ?error=save.
                            if let Some(window) = web_sys::window() {
                                let _ = window.location().reload();
                            }
                            break;
                        }
                        delay = 1500;
                    }
                    Ok(None) => break,
                    Err(_) => delay = delay.saturating_mul(2).min(30_000),
                }
            }
        });
    }
    view! {
        <section class="notice" data-testid="addon-import-landing" role="status">
            <h2>{tr(TextKey::AddonWorkspaceImporting)}</h2>
            <p>{tr(TextKey::AddonWorkspaceImportingHelp)}</p>
            <a class="button secondary" href=format!("/media/addon-lists/{list_id}")>{tr(TextKey::Refresh)}</a>
            <a class="button secondary" href=format!("/media/addon-lists/{list_id}/activity")>{tr(TextKey::AddonWorkspaceActivity)}</a>
        </section>
    }
}
