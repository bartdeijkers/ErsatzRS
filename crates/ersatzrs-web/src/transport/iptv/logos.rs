use std::collections::HashMap;

use axum::{
    extract::Query,
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
};

pub async fn get_generated_logo(Query(query): Query<HashMap<String, String>>) -> Response {
    let text = query.get("text").map_or("TV", String::as_str);
    let svg = generated_logo_svg(text);

    let mut response = svg.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml; charset=utf-8"),
    );
    response
}

#[must_use]
fn generated_logo_svg(text: &str) -> String {
    let label = ersatzrs_infra::channel_logo::label(text);
    let escaped = escape_xml(&label);

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180" viewBox="0 0 320 180" role="img" aria-label="{escaped}"><rect width="320" height="180" fill="#050505"/><rect x="10" y="10" width="300" height="160" fill="none" stroke="#009688" stroke-width="4"/><text x="160" y="104" text-anchor="middle" font-family="Roboto, Arial, sans-serif" font-size="46" font-weight="700" fill="#f5f5f5">{escaped}</text></svg>"##
    )
}

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_logo_uses_initials_for_multi_word_names() {
        assert!(generated_logo_svg("Movie Channel").contains(">MC<"));
    }

    #[test]
    fn generated_logo_escapes_user_text() {
        let svg = generated_logo_svg("A&B");

        assert!(svg.contains("A&amp;B"));
    }
}
