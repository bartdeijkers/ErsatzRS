use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResultRow {
    pub id: i64,
    pub title: String,
    pub media_kind: String,
    pub year: Option<i32>,
    pub duration_seconds: i64,
}

#[server]
pub async fn search_media(query: String) -> Result<Vec<SearchResultRow>, ServerFnError> {
    use crate::state::AppState;
    use chrono::Datelike;
    use ersatzrs_core::scheduling::SchedulingContentSource;

    let query = query.trim().to_owned();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let state = expect_context::<AppState>();
    let mut rows = state
        .repositories
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(query))
        .await?
        .into_iter()
        .map(|item| SearchResultRow {
            id: item.id,
            title: item
                .title
                .unwrap_or_else(|| format!("Media Item {}", item.id)),
            media_kind: media_kind_label(item.show_id, item.season_id, item.season, item.episode),
            year: item.release_date.map(|date| date.year()),
            duration_seconds: item.duration.num_seconds(),
        })
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| {
        left.title
            .cmp(&right.title)
            .then_with(|| left.year.cmp(&right.year))
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(rows)
}

fn media_kind_label(
    show_id: Option<i64>,
    season_id: Option<i64>,
    season: Option<i32>,
    episode: Option<i32>,
) -> String {
    match (show_id, season_id, season, episode) {
        (Some(_), Some(_), Some(season), Some(episode)) => {
            format!("Episode S{season:02}E{episode:02}")
        }
        (Some(_), Some(_), Some(season), None) => format!("Season {season}"),
        (Some(_), _, _, _) => "TV".to_owned(),
        _ => "Media".to_owned(),
    }
}
