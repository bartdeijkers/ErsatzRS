use axum::{
    extract::{Form, Query, State},
    response::Redirect,
};
use ersatzrs_core::search::{SearchExpression, SearchField, SearchTerm, SearchValue};
use serde::Deserialize;

use crate::admin::server_fns::search::{save_saved_search_with_state, SavedSearchForm};
use crate::host::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct QuickFilterForm {
    #[serde(default)]
    base_query: String,
    #[serde(default)]
    media_kind: String,
    #[serde(default)]
    genre: String,
    #[serde(default)]
    year_from: String,
    #[serde(default)]
    year_to: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct SavedSearchFallbackForm {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    return_to: String,
    #[serde(default)]
    saved_search_id: i64,
    #[serde(default)]
    saved_search_name: String,
}

pub async fn apply_quick_filters(Query(input): Query<QuickFilterForm>) -> Redirect {
    let query = build_quick_filter_query(&input);
    if query.is_empty() {
        Redirect::to("/search")
    } else {
        Redirect::to(&format!("/search?query={}", urlencoding::encode(&query)))
    }
}

pub async fn load_saved_search_form(
    State(state): State<AppState>,
    Form(input): Form<SavedSearchFallbackForm>,
) -> Redirect {
    let saved = if input.saved_search_id > 0 {
        state
            .application
            .get_saved_search(input.saved_search_id)
            .await
    } else {
        Ok(None)
    };
    match saved {
        Ok(Some(saved)) => Redirect::to(&saved_search_composer_target(
            &input,
            &saved.query,
            saved.id,
            &saved.name,
            "loaded=search",
        )),
        Ok(None) => Redirect::to(&saved_search_composer_target(
            &input,
            &input.query,
            input.saved_search_id,
            &input.saved_search_name,
            "saved_error=load",
        )),
        Err(error) => {
            tracing::error!(
                %error,
                saved_search_id = input.saved_search_id,
                "failed to load saved search from HTML form"
            );
            Redirect::to(&saved_search_composer_target(
                &input,
                &input.query,
                input.saved_search_id,
                &input.saved_search_name,
                "saved_error=load",
            ))
        }
    }
}

pub async fn save_saved_search_form(
    State(state): State<AppState>,
    Form(input): Form<SavedSearchFallbackForm>,
) -> Redirect {
    let form = SavedSearchForm {
        id: input.saved_search_id,
        name: input.saved_search_name.clone(),
        query: input.query.clone(),
    };
    match save_saved_search_with_state(&state, form).await {
        Ok(id) => Redirect::to(&saved_search_composer_target(
            &input,
            &input.query,
            id,
            input.saved_search_name.trim(),
            "saved=search",
        )),
        Err(error) => Redirect::to(&saved_search_composer_target(
            &input,
            &input.query,
            input.saved_search_id,
            &input.saved_search_name,
            &format!(
                "saved_error={}",
                saved_search_error_code(&error.to_string())
            ),
        )),
    }
}

fn saved_search_composer_target(
    input: &SavedSearchFallbackForm,
    query: &str,
    saved_search_id: i64,
    saved_search_name: &str,
    status: &str,
) -> String {
    let route = if input.id > 0 {
        format!("/media/smart-collections/{}/edit", input.id)
    } else {
        "/media/smart-collections/add".to_owned()
    };
    format!(
        "{route}?query={}&collection_name={}&return_to={}&saved_search_id={saved_search_id}&saved_search_name={}&{status}",
        urlencoding::encode(query),
        urlencoding::encode(&input.name),
        urlencoding::encode(&safe_search_return_path(&input.return_to)),
        urlencoding::encode(saved_search_name),
    )
}

fn safe_search_return_path(candidate: &str) -> String {
    if candidate == "/search" || candidate.starts_with("/search?") {
        candidate.to_owned()
    } else {
        "/search".to_owned()
    }
}

fn saved_search_error_code(error: &str) -> &'static str {
    if error.contains("Saved search name is required") {
        "name_required"
    } else if error.contains("Saved search name must be 80 characters or less") {
        "name_too_long"
    } else if error.contains("Saved search name must be unique") {
        "name_unique"
    } else if error.contains("search_query.rule_required")
        || error.contains("every search rule needs a field and value")
    {
        "query_rule_required"
    } else if error.contains("search_query.group_required") {
        "query_group_required"
    } else if error.contains("search_query.wildcard_required") {
        "query_wildcard_required"
    } else if error.contains("search query is required") || error.contains("search_query.required")
    {
        "query_required"
    } else {
        "save"
    }
}

fn build_quick_filter_query(input: &QuickFilterForm) -> String {
    let mut expressions = Vec::new();
    let base_query = input.base_query.trim();
    if !base_query.is_empty() {
        expressions.push(SearchExpression::parse_user_query(base_query));
    }

    let media_kind = input.media_kind.trim();
    if matches!(
        media_kind,
        "movie"
            | "show"
            | "season"
            | "episode"
            | "artist"
            | "music_video"
            | "other_video"
            | "song"
            | "image"
            | "remote_stream"
    ) {
        expressions.push(text_term(SearchField::MediaKind, media_kind));
    }

    let genre = input.genre.trim();
    if !genre.is_empty() {
        let value = if genre.chars().any(char::is_whitespace) {
            SearchValue::Phrase {
                value: genre.to_owned(),
                slop: 0,
            }
        } else {
            SearchValue::Text(genre.to_owned())
        };
        expressions.push(SearchExpression::Term(SearchTerm {
            field: SearchField::Genre,
            value,
        }));
    }

    let year_from = valid_year(&input.year_from);
    let year_to = valid_year(&input.year_to);
    if year_from.is_some() || year_to.is_some() {
        expressions.push(SearchExpression::Term(SearchTerm {
            field: SearchField::Year,
            value: SearchValue::Range {
                start: year_from,
                end: year_to,
                inclusive: true,
            },
        }));
    }

    match expressions.len() {
        0 => String::new(),
        1 => expressions.remove(0).to_query_string(),
        _ => SearchExpression::And(expressions).to_query_string(),
    }
}

fn text_term(field: SearchField, value: &str) -> SearchExpression {
    SearchExpression::Term(SearchTerm {
        field,
        value: SearchValue::Text(value.to_owned()),
    })
}

fn valid_year(value: &str) -> Option<String> {
    let value = value.trim();
    value
        .parse::<i32>()
        .ok()
        .filter(|year| (1..=9999).contains(year))
        .map(|year| year.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fallback_form() -> SavedSearchFallbackForm {
        SavedSearchFallbackForm {
            id: 17,
            name: "Current collection".to_owned(),
            query: "title:old".to_owned(),
            return_to: "/search?query=genre%3Adrama".to_owned(),
            saved_search_id: 4,
            saved_search_name: "Reusable drama".to_owned(),
        }
    }

    #[test]
    fn quick_filters_build_one_canonical_query() {
        let query = build_quick_filter_query(&QuickFilterForm {
            base_query: "space OR stars".to_owned(),
            media_kind: "movie".to_owned(),
            genre: "Science Fiction".to_owned(),
            year_from: "1980".to_owned(),
            year_to: "1999".to_owned(),
        });

        assert_eq!(
            query,
            "(space OR stars) AND type:movie AND genre:\"Science Fiction\" AND year:[1980 TO 1999]"
        );
    }

    #[test]
    fn quick_filters_ignore_unknown_kind_and_invalid_years() {
        let query = build_quick_filter_query(&QuickFilterForm {
            base_query: "title:space".to_owned(),
            media_kind: "everything".to_owned(),
            genre: String::new(),
            year_from: "0".to_owned(),
            year_to: "later".to_owned(),
        });

        assert_eq!(query, "title:space");
    }

    #[test]
    fn saved_search_redirect_uses_the_recalled_query_and_preserves_editor_state() {
        assert_eq!(
            saved_search_composer_target(
                &fallback_form(),
                "title:recalled AND year:[1980 TO 1999]",
                8,
                "Recalled search",
                "loaded=search",
            ),
            "/media/smart-collections/17/edit?query=title%3Arecalled%20AND%20year%3A%5B1980%20TO%201999%5D&collection_name=Current%20collection&return_to=%2Fsearch%3Fquery%3Dgenre%253Adrama&saved_search_id=8&saved_search_name=Recalled%20search&loaded=search"
        );
    }

    #[test]
    fn saved_search_redirect_rejects_an_external_return_path() {
        let mut input = fallback_form();
        input.return_to = "//example.test".to_owned();
        assert!(
            saved_search_composer_target(&input, "title:safe", 4, "Safe", "saved=search",)
                .contains("return_to=%2Fsearch")
        );
    }
}
