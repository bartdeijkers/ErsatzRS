mod xml;

use xml::{append_fragment, sanitize_xml, validate_xml};

use std::collections::HashMap;
use std::io::Cursor;

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{ChannelRepository, ConfigRepository};
use ersatzrs_core::xmltv::{IconSizeHint, XmltvProgramme, XmltvTimeZone};
use ersatzrs_domain::{Channel, CONFIG_KEY_XMLTV_DAYS_TO_BUILD, CONFIG_KEY_XMLTV_TIME_ZONE};
use ersatzrs_infra::xmltv_templates::{
    self, ChannelGuideTemplates, RenderChannelInput, RenderProgrammeInput,
};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::writer::Writer;

use crate::api::artwork::{artwork_reference_url, ArtworkUrlKind};
use crate::host::state::AppState;
use crate::transport::iptv::access_token::AccessToken;
use crate::transport::iptv::hls::{request_host, request_scheme};
use crate::transport::iptv::identifier::{channel_identifier, parse_channel_number};
use crate::transport::iptv::playlist::{channel_logo_url, load_channel_logo_paths, PlaylistQuery};

const DEFAULT_GUIDE_WINDOW_DAYS: i64 = 2;

pub async fn get_xmltv_xml(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PlaylistQuery>,
) -> Response {
    let channels = match state.repositories.channel.list_enabled().await {
        Ok(channels) => channels,
        Err(error) => {
            tracing::error!(%error, "failed to load channels for xmltv guide");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let days_to_build = load_guide_window_days(&state).await;
    let time_zone = load_guide_time_zone(&state).await;

    let now = Utc::now();
    let horizon = now + Duration::days(days_to_build);
    let programmes =
        match ersatzrs_infra::xmltv::load_xmltv_programmes(&state.pool, now, horizon).await {
            Ok(programmes) => programmes,
            Err(error) => {
                tracing::error!(%error, "failed to load playout items for xmltv guide");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let logo_paths = match load_channel_logo_paths(&state.pool).await {
        Ok(paths) => paths,
        Err(error) => {
            tracing::warn!(%error, "failed to load channel logo paths; using generated logos");
            HashMap::new()
        }
    };

    let templates = match xmltv_templates::load_channel_guide_templates().await {
        Ok(templates) => templates,
        Err(error) => {
            tracing::error!(%error, "failed to load channel guide templates");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let scheme = request_scheme(&headers);
    let host = request_host(&headers);
    let access_token = AccessToken::for_request(
        query.access_token,
        state.iptv_access_token_verifier.is_enabled(),
    );

    match build_xmltv(
        &channels,
        &programmes,
        time_zone,
        &templates,
        &logo_paths,
        &scheme,
        &host,
        &access_token,
    ) {
        Ok(body) => {
            let mut response = body.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/xml; charset=utf-8"),
            );
            response
        }
        Err(error) => {
            tracing::error!(%error, "failed to render xmltv xml");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn load_guide_window_days(state: &AppState) -> i64 {
    state
        .repositories
        .config
        .get(CONFIG_KEY_XMLTV_DAYS_TO_BUILD)
        .await
        .ok()
        .flatten()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|days| *days > 0)
        .unwrap_or(DEFAULT_GUIDE_WINDOW_DAYS)
}

async fn load_guide_time_zone(state: &AppState) -> XmltvTimeZone {
    state
        .repositories
        .config
        .get(CONFIG_KEY_XMLTV_TIME_ZONE)
        .await
        .ok()
        .flatten()
        .as_deref()
        .and_then(XmltvTimeZone::parse)
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn build_xmltv(
    channels: &[Channel],
    programmes: &[XmltvProgramme],
    time_zone: XmltvTimeZone,
    templates: &ChannelGuideTemplates,
    logo_paths: &HashMap<i64, String>,
    scheme: &str,
    host: &str,
    access_token: &AccessToken,
) -> Result<String, quick_xml::Error> {
    let mut ordered_channels: Vec<&Channel> = channels.iter().filter(|c| c.show_in_epg).collect();
    ordered_channels.sort_by(|a, b| {
        parse_channel_number(&a.number).total_cmp(&parse_channel_number(&b.number))
    });

    let mut writer = Writer::new(Cursor::new(Vec::<u8>::new()));
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;

    let mut tv_start = BytesStart::new("tv");
    tv_start.push_attribute(("generator-info-name", "ersatzrs"));
    writer.write_event(Event::Start(tv_start))?;

    let token_uri = access_token.query();
    let token_uri_amp = access_token.query_amp();

    for channel in &ordered_channels {
        let channel_id = channel_identifier(&channel.number);
        let icon_url = channel_logo_url(
            scheme,
            host,
            channel,
            logo_paths,
            &token_uri,
            &token_uri_amp,
        );
        let categories = split_categories(channel.categories.as_deref());
        let channel_artwork_path = logo_paths.get(&channel.id).map(String::as_str);
        let channel_name_encoded = urlencoding::encode(&channel.name);

        let mut wrote_channel = false;
        match xmltv_templates::render_channel(
            templates,
            &RenderChannelInput {
                channel_id: &channel_id,
                channel_number: &channel.number,
                channel_name: &channel.name,
                channel_categories: &categories,
                channel_artwork_path,
                channel_name_encoded: &channel_name_encoded,
                channel_icon_url: &icon_url,
            },
        ) {
            Ok(fragment) => match append_fragment(&mut writer, &fragment) {
                Ok(()) => wrote_channel = true,
                Err(error) => {
                    tracing::warn!(%error, channel_id = %channel_id, "channel guide template produced invalid XML");
                }
            },
            Err(error) => {
                tracing::warn!(%error, channel_id = %channel_id, "channel guide template render failed");
            }
        }
        if !wrote_channel {
            write_fallback_channel(&mut writer, &channel_id, &channel.name)?;
        }
    }

    for programme in programmes {
        let channel_id = channel_identifier(&programme.channel_number);
        let programme_start = format_xmltv_time(programme.start, time_zone);
        let programme_stop = format_xmltv_time(programme.stop, time_zone);
        let icon_url = programme
            .content
            .icon_artwork
            .as_ref()
            .and_then(|path| {
                artwork_reference_url(path, artwork_kind_for(programme.content.icon_size_hint))
            })
            .map(|resolved| guide_artwork_url(scheme, host, &resolved, access_token));
        let thumbnail_url = programme
            .content
            .thumbnail_artwork
            .as_ref()
            .and_then(|path| {
                artwork_reference_url(
                    path,
                    artwork_kind_for(programme.content.thumbnail_size_hint),
                )
            })
            .map(|resolved| guide_artwork_url(scheme, host, &resolved, access_token));
        let input = RenderProgrammeInput {
            programme_start: &programme_start,
            programme_stop: &programme_stop,
            channel_id: &channel_id,
            channel_number: &programme.channel_number,
            custom_title: programme.custom_title.as_deref(),
            title: &programme.title,
            content: &programme.content,
            icon_url: icon_url.as_deref(),
            thumbnail_url: thumbnail_url.as_deref(),
        };

        let mut wrote_programme = false;
        match xmltv_templates::render_programme(templates, &input) {
            Ok(fragment) => match append_fragment(&mut writer, &fragment) {
                Ok(()) => wrote_programme = true,
                Err(error) => {
                    tracing::warn!(%error, channel_id = %channel_id, "channel guide template produced invalid XML");
                }
            },
            Err(error) => {
                tracing::warn!(%error, channel_id = %channel_id, "channel guide template render failed");
            }
        }
        if !wrote_programme {
            let fallback_title = programme
                .custom_title
                .as_deref()
                .unwrap_or(&programme.title);
            write_fallback_programme(
                &mut writer,
                &programme_start,
                &programme_stop,
                &channel_id,
                fallback_title,
            )?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("tv")))?;

    let output = String::from_utf8(writer.into_inner().into_inner())
        .map_err(|error| quick_xml::Error::Io(std::sync::Arc::new(std::io::Error::other(error))))?;
    // Defense in depth: never publish a successful response containing broken XML.
    validate_xml(&output)?;
    Ok(output)
}

fn write_fallback_channel(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    channel_id: &str,
    channel_name: &str,
) -> Result<(), quick_xml::Error> {
    let mut channel_start = BytesStart::new("channel");
    channel_start.push_attribute(("id", sanitize_xml(channel_id).as_ref()));
    writer.write_event(Event::Start(channel_start))?;
    writer.write_event(Event::Start(BytesStart::new("display-name")))?;
    writer.write_event(Event::Text(BytesText::new(&sanitize_xml(channel_name))))?;
    writer.write_event(Event::End(BytesEnd::new("display-name")))?;
    Ok(writer.write_event(Event::End(BytesEnd::new("channel")))?)
}

fn write_fallback_programme(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    start: &str,
    stop: &str,
    channel_id: &str,
    title: &str,
) -> Result<(), quick_xml::Error> {
    let mut programme_start = BytesStart::new("programme");
    programme_start.push_attribute(("start", start));
    programme_start.push_attribute(("stop", stop));
    programme_start.push_attribute(("channel", sanitize_xml(channel_id).as_ref()));
    writer.write_event(Event::Start(programme_start))?;
    let mut title_start = BytesStart::new("title");
    title_start.push_attribute(("lang", "en"));
    writer.write_event(Event::Start(title_start))?;
    writer.write_event(Event::Text(BytesText::new(&sanitize_xml(title))))?;
    writer.write_event(Event::End(BytesEnd::new("title")))?;
    Ok(writer.write_event(Event::End(BytesEnd::new("programme")))?)
}

/// Comma-split, trim, and dedupe (case-insensitively) a channel's
/// categories, matching ErsatzTV's `ChannelCategories`.
fn split_categories(raw: Option<&str>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    raw.map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|category| !category.is_empty())
            .filter(|category| seen.insert(category.to_ascii_lowercase()))
            .map(ToOwned::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

/// The SQL layer already resolved which artwork kind `icon_artwork` actually
/// is (poster-then-thumbnail priority, see `IconSizeHint`); this just picks
/// the matching resize budget `artwork_url` applies.
fn artwork_kind_for(hint: IconSizeHint) -> ArtworkUrlKind {
    match hint {
        IconSizeHint::Poster => ArtworkUrlKind::Poster,
        IconSizeHint::Thumbnail => ArtworkUrlKind::Thumbnail,
    }
}

/// `artwork_url` passes external http(s) sources through unchanged and
/// otherwise returns a path relative to this server; XMLTV is fetched by
/// external clients, so relative paths must be made absolute.
fn absolute_url(scheme: &str, host: &str, relative_or_absolute: &str) -> String {
    if relative_or_absolute.starts_with("http://") || relative_or_absolute.starts_with("https://") {
        relative_or_absolute.to_owned()
    } else {
        format!("{scheme}://{host}{relative_or_absolute}")
    }
}

fn guide_artwork_url(scheme: &str, host: &str, resolved: &str, token: &AccessToken) -> String {
    if let Some(artwork) = resolved.strip_prefix("/artwork/") {
        // Use the transport listener's routes, and never attach IPTV credentials
        // to an external image URL or forward them to a provider.
        format!("{scheme}://{host}/iptv/artwork/{artwork}{}", token.query())
    } else {
        absolute_url(scheme, host, resolved)
    }
}

fn format_xmltv_time(value: DateTime<Utc>, time_zone: XmltvTimeZone) -> String {
    match time_zone {
        XmltvTimeZone::Utc => value.format("%Y%m%d%H%M%S +0000").to_string(),
        XmltvTimeZone::Local => value
            .with_timezone(&chrono::Local)
            .format("%Y%m%d%H%M%S %z")
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ersatzrs_core::xmltv::{XmltvItemKind, XmltvProgrammeContent};
    use ersatzrs_domain::Channel;
    use uuid::Uuid;

    #[test]
    fn guide_artwork_uses_transport_routes_without_leaking_tokens_to_external_images() {
        let token = AccessToken::new(Some("fixture-token"));
        assert_eq!(
            guide_artwork_url(
                "https",
                "example.test",
                "/artwork/posters/jellyfin/42",
                &token
            ),
            "https://example.test/iptv/artwork/posters/jellyfin/42?access_token=fixture-token"
        );
        assert_eq!(
            guide_artwork_url(
                "https",
                "example.test",
                "https://images.example.test/poster.jpg",
                &token
            ),
            "https://images.example.test/poster.jpg"
        );
    }

    fn fixture_channel(number: &str, name: &str) -> Channel {
        let mut channel = Channel::new(Uuid::nil());
        channel.number = number.to_string();
        channel.name = name.to_string();
        channel
    }

    async fn fixture_templates() -> ChannelGuideTemplates {
        let root = tempfile::tempdir().expect("temp templates root");
        ersatzrs_infra::xmltv_templates::load_channel_guide_templates_in(root.path())
            .await
            .expect("load bundled templates")
    }

    #[test]
    fn append_fragment_does_not_partially_write_a_malformed_fragment() {
        let mut writer = Writer::new(Cursor::new(Vec::<u8>::new()));
        let malformed = "<channel id=\"BAD\"><display-name>oops<unterminated";

        let result = append_fragment(&mut writer, malformed);

        assert!(result.is_err());
        let written = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(
            written.is_empty(),
            "a failed parse must not leave partial content in the writer, got: {written}"
        );
    }

    #[test]
    fn append_fragment_rejects_a_token_complete_unclosed_element() {
        let mut writer = Writer::new(Cursor::new(Vec::<u8>::new()));
        let unclosed = "<programme><title>x</title>";

        let result = append_fragment(&mut writer, unclosed);

        assert!(result.is_err());
        assert!(writer.into_inner().into_inner().is_empty());
    }

    #[tokio::test]
    async fn build_xmltv_falls_back_cleanly_when_a_custom_template_is_malformed() {
        let root = tempfile::tempdir().expect("temp templates root");
        tokio::fs::write(
            root.path().join("movie.jinja"),
            "<programme><title>{{ movie_title }}</title>",
        )
        .await
        .unwrap();
        let templates =
            ersatzrs_infra::xmltv_templates::load_channel_guide_templates_in(root.path())
                .await
                .expect("load templates with malformed override");

        let channels = vec![fixture_channel("1", "Movies")];
        let programmes = vec![XmltvProgramme {
            channel_number: "1".into(),
            start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap(),
            stop: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
            custom_title: None,
            title: "Big Buck Bunny".into(),
            content: XmltvProgrammeContent::default(),
        }];

        let output = build_xmltv(
            &channels,
            &programmes,
            XmltvTimeZone::Utc,
            &templates,
            &HashMap::new(),
            "http",
            "example.test",
            &AccessToken::default(),
        )
        .expect("xmltv render");

        roxmltree::Document::parse(&output)
            .expect("the complete output must remain well-formed XML");
        assert!(output.contains("<title lang=\"en\">Big Buck Bunny</title>"));
        assert!(!output.contains("<programme><title>Big Buck Bunny</title>"));
    }

    #[tokio::test]
    async fn build_xmltv_matches_snapshot() {
        let templates = fixture_templates().await;
        let channels = vec![fixture_channel("2", "Kids"), fixture_channel("1", "Movies")];
        let programmes = vec![
            XmltvProgramme {
                channel_number: "1".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
                custom_title: None,
                title: "Big Buck Bunny".into(),
                content: XmltvProgrammeContent::default(),
            },
            XmltvProgramme {
                channel_number: "1".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 22, 0, 0).unwrap(),
                custom_title: None,
                title: "Sintel".into(),
                content: XmltvProgrammeContent::default(),
            },
            XmltvProgramme {
                channel_number: "2".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 20, 30, 0).unwrap(),
                custom_title: None,
                title: "Pingu".into(),
                content: XmltvProgrammeContent::default(),
            },
            XmltvProgramme {
                channel_number: "2".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 30, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
                custom_title: None,
                title: "Unknown".into(),
                content: XmltvProgrammeContent::default(),
            },
        ];

        let output = build_xmltv(
            &channels,
            &programmes,
            XmltvTimeZone::Utc,
            &templates,
            &HashMap::new(),
            "http",
            "example.test",
            &AccessToken::default(),
        )
        .expect("xmltv render");
        // Normalize template-only blank lines for a whitespace-clean snapshot.
        // Metadata whitespace preservation has separate exact-value assertions.
        let output = output
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        insta::assert_snapshot!(output);
    }

    #[tokio::test]
    async fn build_xmltv_includes_rich_movie_and_episode_fields() {
        let templates = fixture_templates().await;
        let channels = vec![fixture_channel("1", "Movies")];
        let movie_content = XmltvProgrammeContent {
            kind: XmltvItemKind::Movie,
            description: Some("A large rabbit has a larger problem.".into()),
            year: Some(2008),
            genres: vec!["Animation".into()],
            content_rating: Some("us:G".into()),
            icon_artwork: Some(ersatzrs_core::repositories::ArtworkReference {
                id: 1,
                kind: ersatzrs_domain::ArtworkKind::Poster,
                path: "/media/movies/Big Buck Bunny/poster.jpg".into(),
            }),
            ..Default::default()
        };
        let episode_content = XmltvProgrammeContent {
            kind: XmltvItemKind::Episode,
            sub_title: Some("Pilot".into()),
            genres: vec!["Drama".into()],
            content_rating: Some("us:TV-14".into()),
            season_number: Some(1),
            episode_number: Some(1),
            ..Default::default()
        };
        let programmes = vec![
            XmltvProgramme {
                channel_number: "1".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
                custom_title: None,
                title: "Big Buck Bunny".into(),
                content: movie_content,
            },
            XmltvProgramme {
                channel_number: "1".into(),
                start: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
                stop: Utc.with_ymd_and_hms(2026, 4, 17, 21, 30, 0).unwrap(),
                custom_title: None,
                title: "Some Show".into(),
                content: episode_content,
            },
        ];

        let output = build_xmltv(
            &channels,
            &programmes,
            XmltvTimeZone::Utc,
            &templates,
            &HashMap::new(),
            "http",
            "example.test",
            &AccessToken::default(),
        )
        .expect("xmltv render");

        assert!(output.contains("<desc lang=\"en\">A large rabbit has a larger problem.</desc>"));
        assert!(output.contains("<date>2008</date>"));
        assert!(output.contains("<category lang=\"en\">Animation</category>"));
        assert!(output.contains("<rating system=\"MPAA\"><value>G</value></rating>"));
        assert!(output.contains(&format!(
            "http:&#x2f;&#x2f;example.test&#x2f;iptv&#x2f;artwork&#x2f;posters&#x2f;{}",
            urlencoding::encode("/media/movies/Big Buck Bunny/poster.jpg")
        )));
        assert!(output.contains("<sub-title lang=\"en\">Pilot</sub-title>"));
        assert!(output.contains("<episode-num system=\"onscreen\">S01E01</episode-num>"));
        assert!(output.contains("<rating system=\"VCHIP\"><value>TV-14</value></rating>"));
    }

    #[tokio::test]
    async fn xmltv_sanitizes_every_bundled_kind_without_losing_rich_metadata() {
        let templates = fixture_templates().await;
        let dirty = "café 日本語 😀 & < > \" ' \u{0}\u{b}\u{fffe}\u{ffff}";
        let clean = sanitize_xml(dirty);
        let mut channel = fixture_channel("1", dirty);
        channel.categories = Some(dirty.into());
        let programmes: Vec<_> = [
            XmltvItemKind::Movie,
            XmltvItemKind::Episode,
            XmltvItemKind::MusicVideo,
            XmltvItemKind::Song,
            XmltvItemKind::OtherVideo,
            XmltvItemKind::RemoteStream,
        ]
        .into_iter()
        .map(|kind| XmltvProgramme {
            channel_number: "1".into(),
            start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap(),
            stop: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
            custom_title: None,
            title: dirty.into(),
            content: XmltvProgrammeContent {
                kind,
                sub_title: Some(dirty.into()),
                description: Some(dirty.into()),
                genres: vec![dirty.into()],
                artists: vec![dirty.into()],
                year: Some(2026),
                ..Default::default()
            },
        })
        .collect();
        let output = build_xmltv(
            &[channel],
            &programmes,
            XmltvTimeZone::Utc,
            &templates,
            &HashMap::new(),
            "http",
            "example.test",
            &AccessToken::default(),
        )
        .unwrap();
        let doc = roxmltree::Document::parse(&output).unwrap();
        assert_eq!(
            doc.descendants()
                .filter(|n| n.has_tag_name("programme"))
                .count(),
            6
        );
        for programme in doc.descendants().filter(|n| n.has_tag_name("programme")) {
            assert_eq!(
                programme
                    .children()
                    .find(|n| n.has_tag_name("title"))
                    .unwrap()
                    .text(),
                Some(clean.as_ref())
            );
            // A fallback would discard these fields. Dirty input must retain the rich guide.
            assert!(programme
                .children()
                .any(|n| n.has_tag_name("date") || n.has_tag_name("desc")));
        }
        assert!(doc
            .descendants()
            .any(|n| n.has_tag_name("display-name") && n.text() == Some(clean.as_ref())));
        assert!(!output.contains(['\u{0}', '\u{b}', '\u{fffe}', '\u{ffff}']));
        assert!(
            programmes.iter().all(|p| p.title == dirty),
            "stored/input metadata is untouched"
        );
    }

    #[tokio::test]
    async fn xmltv_malformed_templates_fall_back_with_sanitized_text_and_attributes() {
        let root = tempfile::tempdir().unwrap();
        // Well-tokenized fragments which the old quick-xml-only check accepted.
        tokio::fs::write(
            root.path().join("channel.jinja"),
            "<channel id=\"1\" id=\"2\"/>",
        )
        .await
        .unwrap();
        tokio::fs::write(
            root.path().join("movie.jinja"),
            "<programme><title>&#xFFFE;</title></programme>",
        )
        .await
        .unwrap();
        let templates = xmltv_templates::load_channel_guide_templates_in(root.path())
            .await
            .unwrap();
        let dirty = "A & < > \" ' 日本語\u{fffe}";
        let channel = fixture_channel("1\u{0}&\"", dirty);
        let programme = XmltvProgramme {
            channel_number: channel.number.clone(),
            start: Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap(),
            stop: Utc.with_ymd_and_hms(2026, 4, 17, 21, 0, 0).unwrap(),
            custom_title: Some(dirty.into()),
            title: "Original title".into(),
            content: XmltvProgrammeContent::default(),
        };
        let output = build_xmltv(
            &[channel],
            &[programme],
            XmltvTimeZone::Utc,
            &templates,
            &HashMap::new(),
            "http",
            "example.test",
            &AccessToken::default(),
        )
        .unwrap();
        let doc = roxmltree::Document::parse(&output).unwrap();
        let channel = doc
            .descendants()
            .find(|n| n.has_tag_name("channel"))
            .unwrap();
        let programme = doc
            .descendants()
            .find(|n| n.has_tag_name("programme"))
            .unwrap();
        assert_eq!(channel.attribute("id"), programme.attribute("channel"));
        assert_eq!(
            channel.first_element_child().unwrap().text(),
            Some(sanitize_xml(dirty).as_ref())
        );
        assert_eq!(
            programme.first_element_child().unwrap().text(),
            Some(sanitize_xml(dirty).as_ref())
        );
        assert_eq!(
            doc.root_element()
                .children()
                .filter(roxmltree::Node::is_element)
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn xmltv_empty_guide_is_a_complete_xml_document() {
        let output = build_xmltv(
            &[],
            &[],
            XmltvTimeZone::Utc,
            &fixture_templates().await,
            &HashMap::new(),
            "http",
            "example.test",
            &AccessToken::default(),
        )
        .unwrap();
        assert_eq!(
            roxmltree::Document::parse(&output)
                .unwrap()
                .root_element()
                .tag_name()
                .name(),
            "tv"
        );
    }

    #[test]
    fn split_categories_trims_dedupes_and_drops_blanks() {
        assert_eq!(
            split_categories(Some(" Film, family , Film,,")),
            vec!["Film".to_owned(), "family".to_owned()]
        );
        assert_eq!(split_categories(None), Vec::<String>::new());
    }

    #[test]
    fn absolute_url_passes_through_external_urls() {
        assert_eq!(
            absolute_url("http", "host", "https://cdn.example/x.png"),
            "https://cdn.example/x.png"
        );
        assert_eq!(
            absolute_url("http", "host:8409", "/artwork/posters/x.jpg"),
            "http://host:8409/artwork/posters/x.jpg"
        );
    }

    #[test]
    fn format_xmltv_time_utc_is_unaffected_by_local_offset() {
        let value = Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap();
        assert_eq!(
            format_xmltv_time(value, XmltvTimeZone::Utc),
            "20260417200000 +0000"
        );
    }

    #[test]
    fn format_xmltv_time_local_uses_signed_local_offset() {
        let value = Utc.with_ymd_and_hms(2026, 4, 17, 20, 0, 0).unwrap();
        let formatted = format_xmltv_time(value, XmltvTimeZone::Local);
        let (datetime_part, offset_part) =
            formatted.split_once(' ').expect("space-separated offset");
        assert_eq!(datetime_part.len(), 14);
        assert!(datetime_part.chars().all(|c| c.is_ascii_digit()));
        assert!(offset_part.starts_with('+') || offset_part.starts_with('-'));
        assert_eq!(offset_part.len(), 5);
        assert!(offset_part[1..].chars().all(|c| c.is_ascii_digit()));
    }
}
