//! Read-only current guide view, separate from immutable draft editor controls.

use ersatzrs_domain::{TeletextPageAddress, TeletextPageNumber, TeletextSubcode};
use leptos::prelude::*;
use leptos_router::params::ParamsMap;

use crate::admin::{
    i18n::{tr, TextKey},
    server_fns::teletext_drafts::TeletextCurrentGuidePreviewData,
};

use super::teletext_drafts::style;

pub(super) fn query(values: &ParamsMap) -> Option<Result<TeletextPageAddress, ()>> {
    values.get("guide_preview")?;
    Some((|| {
        let page = values
            .get("guide_page")
            .ok_or(())?
            .parse::<u16>()
            .map_err(|_| ())?;
        let subcode = values.get("guide_subcode").ok_or(())?;
        if subcode.len() != 4 {
            return Err(());
        }
        let subcode = u16::from_str_radix(&subcode, 16).map_err(|_| ())?;
        Ok(TeletextPageAddress {
            page: TeletextPageNumber::new(page).map_err(|_| ())?,
            subcode: TeletextSubcode::new(subcode).map_err(|_| ())?,
        })
    })())
}

pub(super) fn view(
    channel_id: i64,
    available: bool,
    result: Option<Result<TeletextCurrentGuidePreviewData, ServerFnError>>,
) -> AnyView {
    let owner = result
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|r| r.owner);
    let content = match result {
        None => ().into_any(),
        Some(Err(_)) => {
            view! { <p role="alert">{tr(TextKey::TeletextAdminUnavailable)}</p> }.into_any()
        }
        Some(Ok(data)) => current(data),
    };
    view! {
        <section class=style::preview data-testid="teletext-current-guide">
            <h2>{tr(TextKey::TeletextGuideCurrentPreview)}</h2>
            <p>{tr(TextKey::TeletextGuideCurrentPreviewHelp)}</p>
            <form class=style::preview_form method="get" action=format!("/channels/{channel_id}/teletext")>
                <input type="hidden" name="guide_preview" value="1"/>
                <fieldset class=style::fields disabled=!available>
                <legend>{tr(TextKey::TeletextGuideCurrentPreview)}</legend>
                <div>
                    <label for="guide-preview-page">{tr(TextKey::TeletextPreviewPage)}</label>
                    <input id="guide-preview-page" name="guide_page" type="number" min="100" max="899" required
                        value=owner.map(|owner| owner.page.get().to_string())/>
                </div>
                <div>
                    <label for="guide-preview-subcode">{tr(TextKey::TeletextPreviewSubpage)}</label>
                    <input id="guide-preview-subcode" name="guide_subcode" pattern="[0-9a-fA-F]{4}" required
                        value=owner.map(|owner| format!("{:04X}", owner.subcode.get()))/>
                </div>
                <button type="submit">{tr(TextKey::TeletextPreviewLoad)}</button>
                </fieldset>
            </form>
            {content}
        </section>
    }.into_any()
}

fn url(channel_id: i64, owner: TeletextPageAddress) -> String {
    format!(
        "/channels/{channel_id}/teletext?guide_preview=1&guide_page={}&guide_subcode={:04X}",
        owner.page.get(),
        owner.subcode.get()
    )
}

fn current(data: TeletextCurrentGuidePreviewData) -> AnyView {
    let channel_id = data.channel_id;
    let generation_id = data.generation.generation_id;
    let snapshot_id = data
        .generation
        .guide_refresh
        .as_ref()
        .and_then(|r| r.snapshot_id);
    let captured = data
        .generation
        .guide_refresh
        .as_ref()
        .map(|r| r.captured_at.to_rfc3339());
    let freshness = tr(
        if data
            .generation
            .guide_refresh
            .as_ref()
            .is_some_and(|r| r.stale)
        {
            TextKey::TeletextGuideStale
        } else {
            TextKey::TeletextGuideFresh
        },
    );
    let pages = data.pages.iter().map(|owner| view! {
        <a href=url(channel_id, *owner) aria-current=(*owner == data.owner).then_some("page")>
            {format!("{}/{:04X}", owner.page.get(), owner.subcode.get())}
        </a>
    }).collect_view();
    let diagnostics = super::teletext_drafts::diagnostic_list(&data.diagnostics);
    let legend = super::teletext_drafts::preview_legend();
    view! {
        <div data-testid="teletext-current-guide-model" data-generation=generation_id data-snapshot=snapshot_id>
            <p role="status">{freshness}" · "{tr(TextKey::TeletextGuideCapturedAt)}" "{captured}</p>
            <nav class=style::inventory aria-label=tr(TextKey::TeletextPreviewPage)>{pages}</nav>
            <img class=style::raster src=data.image_url alt=format!("{}/{:04X}", data.owner.page.get(), data.owner.subcode.get())/>
            {legend}
            {diagnostics}
        </div>
    }.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_guide_requires_explicit_valid_address_and_grants_no_edit_authority() {
        let mut params = ParamsMap::new();
        assert_eq!(query(&params), None);
        params.insert("guide_preview", "1".into());
        assert_eq!(query(&params), Some(Err(())));
        params.insert("guide_page", "410".into());
        params.insert("guide_subcode", "0020".into());
        let address = query(&params).unwrap().unwrap();
        assert_eq!(address.page.get(), 410);
        assert_eq!(address.subcode.get(), 0x20);
        params.insert("guide_subcode", "20".into());
        assert_eq!(query(&params), Some(Err(())));
        let owner = Owner::new();
        let html = owner.with(|| view(77, true, None).to_html());
        assert!(!html.contains("design_id"));
        assert!(!html.contains("publication"));
        assert!(!html.contains("method=\"post\""));
        assert!(html.contains("guide_page"));
    }
}
