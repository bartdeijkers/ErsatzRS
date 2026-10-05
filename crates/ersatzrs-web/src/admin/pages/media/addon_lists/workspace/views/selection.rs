use super::{button, checkbox, input, select, textarea, tr, TextKey, Workspace, FORM};
use ersatzrs_core::addon_list_selection::AddonListSelectionReason as R;
use leptos::prelude::*;

pub(super) fn selection(w: &Workspace) -> AnyView {
    let saved = &w.form.selection.settings;
    let d = &w.submission.draft;
    let policy = saved.policy.clone().unwrap_or_default();
    let preview = w.submission.selection_preview.clone().or_else(|| {
        let state = expect_context::<crate::host::state::AppState>();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                state
                    .application
                    .ports()
                    .addon_media_lists()
                    .preview_selection(w.form.id, saved.policy.clone()),
            )
        })
        .ok()
    });
    view! {
        <section class="form-section" id="selection" data-testid="addon-selection">
            <h2>{tr(TextKey::AddonSelection)}</h2><p>{tr(TextKey::AddonSelectionHelp)}</p>
            {w.form.selection.review_required.then(||view!{<p class="notice" role="status" data-testid="addon-selection-review-required">{tr(TextKey::AddonSelectionReviewRequired)}</p>})}
            <dl class="facts"><div><dt>{tr(TextKey::AddonSelectionEvaluatedAt)}</dt><dd>{w.form.selection.evaluated_at.map(|t|t.to_rfc3339()).unwrap_or_default()}</dd></div><div><dt>{tr(TextKey::AddonSelectionCutoff)}</dt><dd data-testid="addon-selection-saved-cutoff">{w.form.selection.cutoff_at.map(|t|t.to_rfc3339()).unwrap_or_default()}</dd></div></dl>
            {checkbox("selection_enabled",TextKey::AddonSelectionEnabled,d.field("settings","selection_enabled",saved.policy.is_some().to_string())=="true")}
            {checkbox("selection_paused",TextKey::AddonSelectionPause,d.field("settings","selection_paused",saved.refresh_paused.to_string())=="true")}
            <div class="grid-2">{[
                ("selection_age_days",TextKey::AddonSelectionAge,policy.age_days.map(u64::from)),
                ("selection_newest_count",TextKey::AddonSelectionNewest,policy.newest_count.map(u64::from)),
                ("selection_minimum_seconds",TextKey::AddonSelectionMinimum,policy.minimum_seconds),
                ("selection_maximum_seconds",TextKey::AddonSelectionMaximum,policy.maximum_seconds),
            ].into_iter().map(|(name,key,n)| input(name,key,d.field("settings",name,n.map(|n|n.to_string()).unwrap_or_default()),"number")).collect_view()}</div>
            {textarea("selection_include_terms",TextKey::AddonSelectionInclude,d.field("settings","selection_include_terms",policy.include_terms.join("\n")))}
            {textarea("selection_exclude_terms",TextKey::AddonSelectionExclude,d.field("settings","selection_exclude_terms",policy.exclude_terms.join("\n")))}
            {button("preview-selection",TextKey::Preview,false)}
            {preview.map(|p| view! {
                <div data-testid="addon-selection-preview" aria-live="polite">
                    <dl class="facts"><div><dt>{tr(TextKey::AddonSelectionActive)}</dt><dd>{p.accepted_count}</dd></div><div><dt>{tr(TextKey::AddonSelectionArchived)}</dt><dd>{p.decisions.len().saturating_sub(p.accepted_count)}</dd></div><div><dt>{tr(TextKey::AddonSelectionCutoff)}</dt><dd>{p.cutoff_at.map(|t|t.to_rfc3339()).unwrap_or_default()}</dd></div></dl>
                    <p>{tr(TextKey::AddonSelectionPreviewHelp)}</p>
                    <table><thead><tr><th>{tr(TextKey::Title)}</th><th>{tr(TextKey::Status)}</th></tr></thead><tbody>{p.decisions.into_iter().take(100).map(|item|view!{<tr><td><a href=format!("/media/addon-lists/{}/items/{}",w.form.id,item.item_id)>{item.title}</a></td><td>{tr(reason_key(item.reason))}</td></tr>}).collect_view()}</tbody></table>
                </div>
            })}
            <fieldset><legend>{tr(TextKey::AddonSelectionManual)}</legend>
                {select("selection_item_id",TextKey::Title,d.field("settings","selection_item_id",String::new()),w.items.iter().map(|item|(item.id.to_string(),item.display_title.clone())).collect())}
                <button form=FORM name="intent" value="selection-pin" class="button secondary">{tr(TextKey::AddonSelectionPinned)}</button>
                <button form=FORM name="intent" value="selection-exclude" class="button secondary">{tr(TextKey::AddonSelectionExcluded)}</button>
                <button form=FORM name="intent" value="selection-auto" class="button secondary">{tr(TextKey::AddonSelectionAutomatic)}</button>
            </fieldset>
        </section>
    }.into_any()
}

fn reason_key(reason: R) -> TextKey {
    match reason {
        R::Accepted => TextKey::AddonSelectionActive,
        R::Pinned => TextKey::AddonSelectionPinned,
        R::Excluded => TextKey::AddonSelectionExcluded,
        R::MissingDate => TextKey::AddonSelectionMissingDate,
        R::FutureDate => TextKey::AddonSelectionFutureDate,
        R::TooOld => TextKey::AddonSelectionTooOld,
        R::MissingDuration => TextKey::AddonSelectionMissingDuration,
        R::TooShort => TextKey::AddonSelectionTooShort,
        R::TooLong => TextKey::AddonSelectionTooLong,
        R::TitleNotIncluded => TextKey::AddonSelectionTitleNotIncluded,
        R::TitleExcluded => TextKey::AddonSelectionTitleExcluded,
        R::NewestLimit => TextKey::AddonSelectionNewestLimit,
    }
}
