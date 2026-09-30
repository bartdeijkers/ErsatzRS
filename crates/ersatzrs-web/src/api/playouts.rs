use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::Deserialize;

use crate::host::state::AppState;

#[cfg(test)]
#[path = "playout_schedule_tests.rs"]
mod schedule_tests;

#[derive(Debug, Deserialize)]
pub struct PlayoutCreatePostForm {
    #[serde(default)]
    kind: String,
    channel_id: i64,
    #[serde(default)]
    program_schedule_id: Option<i64>,
    #[serde(default)]
    schedule_file: String,
}

pub async fn create_playout_form(
    State(state): State<AppState>,
    Form(input): Form<PlayoutCreatePostForm>,
) -> Redirect {
    let kind = input.kind.clone();
    match state
        .application
        .create_playout(ersatzrs_app::commands::CreatePlayout {
            kind: input.kind,
            channel_id: input.channel_id,
            program_schedule_id: input.program_schedule_id,
            schedule_file: input.schedule_file,
        })
        .await
    {
        Ok(_) => Redirect::to("/playouts"),
        Err(error) => {
            tracing::error!(%error, "failed to create playout from HTML form");
            Redirect::to(&add_error_redirect(&kind))
        }
    }
}

/// Build the `/playouts/add` error redirect for a submitted `kind`.
///
/// `kind` is raw form input, so only recognized kinds are echoed back into the
/// kinded sub-route; anything else (including unknown or crafted values) falls
/// back to the bare add route. This keeps user input out of the `Location`
/// header path segment.
fn add_error_redirect(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().as_str() {
        slug @ ("block" | "sequential" | "scripted" | "externaljson") => {
            format!("/playouts/add/{slug}?error=create")
        }
        // Classic (empty / "classic") is the default add route with no kind
        // segment; unknown or crafted values fall back here too.
        _ => "/playouts/add?error=create".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::add_error_redirect;

    #[test]
    fn known_kinds_redirect_to_their_sub_route() {
        assert_eq!(
            add_error_redirect("block"),
            "/playouts/add/block?error=create"
        );
        // Canonical (capitalized) form from the island's hidden field.
        assert_eq!(
            add_error_redirect("ExternalJson"),
            "/playouts/add/externaljson?error=create"
        );
    }

    #[test]
    fn classic_and_empty_use_the_bare_add_route() {
        assert_eq!(add_error_redirect(""), "/playouts/add?error=create");
        assert_eq!(add_error_redirect("classic"), "/playouts/add?error=create");
    }

    #[test]
    fn unknown_or_crafted_kinds_never_leak_into_the_path() {
        // A crafted value must not be echoed into the Location path segment.
        assert_eq!(
            add_error_redirect("../../evil?x=y"),
            "/playouts/add?error=create"
        );
        assert_eq!(
            add_error_redirect("block/../../evil"),
            "/playouts/add?error=create"
        );
    }
}
