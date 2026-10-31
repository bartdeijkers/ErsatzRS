//! Read-only per-output receipts, deliberately separate from publication forms.

use std::fmt::Write as _;

use super::TeletextDraftPreviewData;
use crate::admin::i18n::{tr, TextKey};
use ersatzrs_domain::{
    TeletextOutputObservationPhase as Phase, TeletextOutputSelectionObservation,
};
use leptos::prelude::*;

pub(super) fn view(data: &TeletextDraftPreviewData) -> AnyView {
    let Some(observations) = &data.outputs else {
        return view! {
            <section data-testid="teletext-output-status" data-state="unavailable">
                <h3>{tr(TextKey::TeletextDeliveryTitle)}</h3>
                <p role="status">{tr(TextKey::TeletextDeliveryUnavailable)}</p>
            </section>
        }
        .into_any();
    };
    let outputs = observations.outputs.iter().enumerate().map(|(index, output)| {
        let (state, key) = match output.phase {
            Phase::Waiting => ("waiting", TextKey::TeletextDeliveryWaiting),
            Phase::Applied => ("applied", TextKey::TeletextDeliveryApplied),
            Phase::Rejected => ("rejected", TextKey::TeletextDeliveryRejected),
        };
        let requested = selection(&output.requested);
        let confirmed = output.applied.as_ref().map_or_else(
            || tr(TextKey::TeletextDeliveryUnconfirmed).to_owned(), selection,
        );
        view! {
            <li data-testid="teletext-output" data-output=output.epoch.to_string() data-state=state
                data-requested-generation=output.requested.generation.as_ref().map(|generation| generation.generation_id)
                data-applied-generation=output.applied.as_ref().and_then(|selection| selection.generation.as_ref()).map(|generation| generation.generation_id)
                data-requested-guide-snapshot=output.requested.generation.as_ref().and_then(|generation| generation.guide_refresh.as_ref()).and_then(|refresh| refresh.snapshot_id)
                data-applied-guide-snapshot=output.applied.as_ref().and_then(|selection| selection.generation.as_ref()).and_then(|generation| generation.guide_refresh.as_ref()).and_then(|refresh| refresh.snapshot_id)>
                <h4>{format!("{} {}", tr(TextKey::TeletextDeliveryLabel), index + 1)}</h4>
                <p role="status">{tr(key)}</p>
                <dl>
                    <dt>{tr(TextKey::TeletextDeliveryRequested)}</dt><dd>{requested}</dd>
                    <dt>{tr(TextKey::TeletextDeliveryConfirmed)}</dt><dd>{confirmed}</dd>
                </dl>
            </li>
        }
    }).collect_view();
    view! {
        <section data-testid="teletext-output-status" data-state="observed">
            <h3>{tr(TextKey::TeletextDeliveryTitle)}</h3>
            <p>{tr(TextKey::TeletextDeliveryHelp)}</p>
            <p role="status" hidden=!observations.outputs.is_empty()>{tr(TextKey::TeletextDeliveryEmpty)}</p>
            <p role="status" hidden=!observations.incomplete>{tr(TextKey::TeletextDeliveryIncomplete)}</p>
            <ul>{outputs}</ul>
        </section>
    }.into_any()
}

fn selection(selection: &TeletextOutputSelectionObservation) -> String {
    let generation = selection.generation.as_ref().map(|generation| {
        let mut label = format!(
            "{} {}",
            tr(TextKey::TeletextDeliveryGeneration),
            generation.generation_id
        );
        if let Some(refresh) = &generation.guide_refresh {
            let snapshot = refresh.snapshot_id.map_or_else(
                || tr(TextKey::TeletextGuideInitialSeed).to_owned(),
                |id| format!("{} {id}", tr(TextKey::TeletextGuideRuntimeSnapshot)),
            );
            let freshness = tr(if refresh.stale {
                TextKey::TeletextGuideStale
            } else {
                TextKey::TeletextGuideFresh
            });
            let _ = write!(
                label,
                " · {snapshot} · {freshness} · {} {}",
                tr(TextKey::TeletextGuideCapturedAt),
                refresh.captured_at.to_rfc3339(),
            );
        }
        label
    });
    if selection.authored {
        generation.unwrap_or_else(|| tr(TextKey::TeletextDeliveryUnavailable).to_owned())
    } else {
        generation.map_or_else(
            || tr(TextKey::TeletextDeliveryAbsent).to_owned(),
            |generation| format!("{generation} · {}", tr(TextKey::TeletextDeliveryAbsent)),
        )
    }
}
