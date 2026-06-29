//! Shared row rendering for TV show/season special features.

use leptos::prelude::*;

use crate::admin::i18n::{content_variant_badge_label, tr, TextKey};
use crate::admin::server_fns::media_browser::MediaCardRow;
use crate::admin::ui::primitives::card::first_letter_placeholder;

stylance::import_crate_style!(style, "src/admin/pages/media/tv_specials.module.css");

#[component]
pub(super) fn TvSpecialFeatureRow(
    extra: MediaCardRow,
    on_add_collection: Callback<i64>,
    on_add_playlist: Callback<i64>,
    on_show_info: Callback<(i64, String)>,
) -> impl IntoView {
    let extra_id = extra.id;
    let title = extra.title;
    let sort_title = extra.sort_title;
    let subtitle = extra.subtitle.filter(|value| !value.trim().is_empty());
    let artwork_path = extra.artwork_path;
    let badge = content_variant_badge_label(
        extra.variant.content_kind,
        extra.variant.sub_kind.as_deref(),
    );
    let href = format!("/media/other/videos/{extra_id}");
    let troubleshoot_href = format!("/system/troubleshooting/playback?mediaItem={extra_id}");
    let placeholder = first_letter_placeholder(&sort_title);
    let thumbnail_alt = format!("{title} {}", tr(TextKey::Thumbnail));
    let article_label = title.clone();
    let title_for_link = title.clone();
    let title_for_info = title.clone();

    let thumb = match artwork_path {
        Some(src) => view! {
            <img class=style::thumb src=src alt=thumbnail_alt loading="lazy"/>
        }
        .into_any(),
        None => view! {
            <div class=style::special_thumb_placeholder>{placeholder}</div>
        }
        .into_any(),
    };

    view! {
        <article
            class=style::episode_card
            id=format!("special-{extra_id}")
            aria-label=article_label
            data-media-special-row="true"
        >
            <div class=style::episode_main>
                <div class=style::thumb_wrap>{thumb}</div>
                <div class=style::episode_body>
                    <div class=style::special_heading>
                        <h2 class=format!("{} {}", style::episode_title, style::special_title)>
                            <a class=style::special_title_link href=href>{title_for_link}</a>
                        </h2>
                        {badge.map(|label| view! {
                            <span class=style::special_badge>{label}</span>
                        })}
                    </div>
                    {subtitle.map(|value| view! {
                        <p class=style::special_subtitle>{value}</p>
                    })}
                    <div class=style::episode_actions>
                        <button
                            type="button"
                            class=style::action_btn
                            on:click=move |_| on_add_collection.run(extra_id)
                        >
                            {tr(TextKey::AddToCollection)}
                        </button>
                        <button
                            type="button"
                            class=style::action_btn
                            on:click=move |_| on_add_playlist.run(extra_id)
                        >
                            {tr(TextKey::AddToPlaylist)}
                        </button>
                        <button
                            type="button"
                            class=style::secondary_btn
                            on:click=move |_| on_show_info.run((extra_id, title_for_info.clone()))
                        >
                            {tr(TextKey::ShowMediaInfo)}
                        </button>
                        <a class=style::secondary_btn href=troubleshoot_href>
                            {tr(TextKey::TroubleshootPlayback)}
                        </a>
                    </div>
                </div>
            </div>
        </article>
    }
}
