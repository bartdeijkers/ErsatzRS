//! Custom pages island on the channel teletext page.
//!
//! Custom pages come from the teletext element library. Until that library
//! links elements to channels, this island shows its empty state and adds no
//! page-set rows. The island keeps the shared layout so the library can fill
//! it without changing the rest of the channel page.

use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::teletext::ChannelTeletextData;
use crate::admin::ui::primitives::teletext::{island, status_badge, TeletextIsland, TeletextTone};

use super::teletext::style;

/// Page-set rows contributed by custom pages linked to this channel.
pub(super) fn page_set_rows(_data: &ChannelTeletextData) -> Vec<AnyView> {
    Vec::new()
}

pub(super) fn custom_pages_island(_data: &ChannelTeletextData) -> AnyView {
    island(TeletextIsland {
        test_id: "teletext-custom-pages",
        title: tr(TextKey::TeletextCustomPagesTitle).to_owned(),
        badge: Some(status_badge(
            TeletextTone::Neutral,
            tr(TextKey::TeletextCustomPagesNone),
        )),
        help: Some(tr(TextKey::TeletextCustomPagesHelp).to_owned()),
        data_id: None,
        body: view! {
            <div class=style::empty>
                <p><strong>{tr(TextKey::TeletextCustomPagesEmpty)}</strong></p>
                <p>{tr(TextKey::TeletextCustomPagesEmptyHelp)}</p>
            </div>
        }
        .into_any(),
    })
}
