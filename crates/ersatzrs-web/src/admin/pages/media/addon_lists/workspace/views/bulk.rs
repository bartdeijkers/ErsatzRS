use leptos::prelude::*;
use std::collections::BTreeMap;

use super::{content_kind_value, tr, value, TextKey, Workspace};

pub(super) fn preview(w: &Workspace) -> AnyView {
    let d = &w.submission.draft;
    let Some(previous) = &d.undo else {
        return ().into_any();
    };
    let empty = BTreeMap::new();
    let before = previous.get("items").unwrap_or(&empty);
    let after = d.groups.get("items").unwrap_or(&empty);
    let rows = w.items.iter().filter_map(|row| {
        let fields = [
            ("included",TextKey::IncludeInPublication,row.included.to_string()),
            ("season",TextKey::Season,row.season.map(|n|n.to_string()).unwrap_or_default()),
            ("episode",TextKey::Episode,row.episode.map(|n|n.to_string()).unwrap_or_default()),
            ("kind",TextKey::MediaKind,content_kind_value(row.content_kind).into()),
            ("liveness",TextKey::PlaybackBehavior,row.liveness_override.map_or_else(||"inherit".into(),value)),
        ];
        let mut old = Vec::new();
        let mut new = Vec::new();
        for (field,label,saved) in fields {
            let key = format!("{field}_{}",row.id);
            let a = before.get(&key).unwrap_or(&saved);
            let b = after.get(&key).unwrap_or(&saved);
            if a != b {
                old.push(format!("{}: {}",tr(label),display_value(field,a)));
                new.push(format!("{}: {}",tr(label),display_value(field,b)));
            }
        }
        (!new.is_empty()).then(||view!{<tr><th scope="row">{row.display_title.clone()}</th><td>{old.join(" · ")}</td><td>{new.join(" · ")}</td></tr>})
    }).collect_view();
    view!{<section class="work-section" data-testid="bulk-preview"><h3>{tr(TextKey::Preview)}</h3><div class="table-scroll"><table><thead><tr><th>{tr(TextKey::Title)}</th><th>{tr(TextKey::AddonOptionsBefore)}</th><th>{tr(TextKey::AddonOptionsAfter)}</th></tr></thead><tbody>{rows}</tbody></table></div></section>}.into_any()
}

fn display_value(field: &str, value: &str) -> String {
    if matches!(field, "season" | "episode") {
        return if value.is_empty() {
            tr(TextKey::Unnumbered).into()
        } else {
            value.into()
        };
    }
    tr(match value {
        "true" => TextKey::Yes,
        "false" => TextKey::No,
        "auto" => TextKey::AddonAutoDetect,
        "television_episode" => TextKey::Episode,
        "movie" => TextKey::Movie,
        "music_video" => TextKey::MusicVideo,
        "other_video" => TextKey::OtherVideos,
        "song" => TextKey::Songs,
        "image" => TextKey::Images,
        "inherit" => TextKey::AddonOptionsInherited,
        "finite" => TextKey::FiniteMedia,
        "live" => TextKey::LiveSource,
        _ => TextKey::UnknownAvailability,
    })
    .into()
}
