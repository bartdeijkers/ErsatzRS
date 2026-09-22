use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use ersatzrs_core::repositories::ChannelRepository;
use ersatzrs_domain::Channel;
use serde::Serialize;
use uuid::Uuid;

use crate::host::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[allow(non_snake_case)]
pub struct Discover {
    pub DeviceAuth: String,
    pub DeviceID: String,
    pub FirmwareName: String,
    pub FirmwareVersion: String,
    pub FriendlyName: String,
    pub LineupURL: String,
    pub Manufacturer: String,
    pub ManufacturerURL: String,
    pub ModelNumber: String,
    pub TunerCount: i32,
}

impl Discover {
    #[must_use]
    pub fn new(scheme: &str, host: &str, tuner_count: i32, uuid: Uuid) -> Self {
        Self {
            DeviceAuth: String::new(),
            DeviceID: uuid.to_string(),
            FirmwareName: "hdhomeruntc_atsc".to_string(),
            FirmwareVersion: "20190621".to_string(),
            FriendlyName: "ErsatzRS".to_string(),
            LineupURL: format!("{scheme}://{host}/lineup.json"),
            Manufacturer: "ErsatzRS".to_string(),
            ManufacturerURL: "https://github.com/bartdeijkers/ErsatzRS".to_string(),
            ModelNumber: "HDTC-2US".to_string(),
            TunerCount: tuner_count,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[allow(non_snake_case)]
pub struct LineupStatus {
    pub ScanInProgress: i32,
    pub ScanPossible: i32,
    pub Source: String,
    pub SourceList: Vec<String>,
}

impl Default for LineupStatus {
    fn default() -> Self {
        Self {
            ScanInProgress: 0,
            ScanPossible: 1,
            Source: "Cable".to_string(),
            SourceList: vec!["Cable".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[allow(non_snake_case)]
pub struct LineupItem {
    pub GuideNumber: String,
    pub GuideName: String,
    pub URL: String,
}

impl LineupItem {
    #[must_use]
    pub fn from_channel(scheme: &str, host: &str, channel: &Channel) -> Self {
        Self {
            GuideNumber: channel.number.clone(),
            GuideName: channel.name.clone(),
            URL: format!("{scheme}://{host}/iptv/channel/{}.ts", channel.number),
        }
    }
}

pub async fn get_discover_json(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<Discover> {
    let scheme = request_scheme(&headers);
    let host = request_host(&headers);
    Json(Discover::new(
        &scheme,
        &host,
        state.tuner_count(),
        state.device_uuid,
    ))
}

pub async fn get_lineup_status_json() -> Json<LineupStatus> {
    Json(LineupStatus::default())
}

pub async fn get_device_xml(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let scheme = request_scheme(&headers);
    let host = request_host(&headers);
    let body = render_device_xml(&scheme, &host, state.device_uuid);

    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/xml; charset=utf-8"),
    );
    response
}

pub async fn get_lineup_json(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let channels = match state.repositories.channel.list_enabled().await {
        Ok(channels) => channels,
        Err(error) => {
            tracing::error!(%error, "failed to load channels for lineup.json");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let scheme = request_scheme(&headers);
    let host = request_host(&headers);
    let mut ordered: Vec<&Channel> = channels.iter().collect();
    ordered.sort_by(|a, b| {
        parse_channel_number(&a.number).total_cmp(&parse_channel_number(&b.number))
    });
    let lineup: Vec<LineupItem> = ordered
        .iter()
        .map(|channel| LineupItem::from_channel(&scheme, &host, channel))
        .collect();
    Json(lineup).into_response()
}

fn render_device_xml(scheme: &str, host: &str, uuid: Uuid) -> String {
    format!(
        r#"<root xmlns="urn:schemas-upnp-org:device-1-0">
    <URLBase>{scheme}://{host}</URLBase>
    <specVersion>
        <major>1</major>
        <minor>0</minor>
    </specVersion>
    <device>
        <deviceType>urn:schemas-upnp-org:device:MediaServer:1</deviceType>
        <friendlyName>ErsatzRS</friendlyName>
        <manufacturer>Silicondust</manufacturer>
        <modelName>HDTC-2US</modelName>
        <modelNumber>HDTC-2US</modelNumber>
        <serialNumber>{uuid}</serialNumber>
        <UDN>uuid:{uuid}</UDN>
    </device>
</root>"#
    )
}

fn parse_channel_number(number: &str) -> f64 {
    number.parse::<f64>().unwrap_or(f64::MAX)
}

fn request_scheme(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map_or_else(
            || "http".to_string(),
            |value| value.split(',').next().unwrap_or(value).trim().to_string(),
        )
}

fn request_host(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|value| value.to_str().ok())
        .map_or_else(
            || "localhost".to_string(),
            |value| value.split(',').next().unwrap_or(value).trim().to_string(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_uuid() -> Uuid {
        Uuid::parse_str("11111111-2222-3333-4444-555555555555").expect("valid uuid literal")
    }

    #[test]
    fn discover_json_shape_is_stable() {
        let discover = Discover::new("http", "example.test:8080", 2, fixture_uuid());
        let json = serde_json::to_string_pretty(&discover).expect("serialize discover");
        insta::assert_snapshot!(json);
    }

    #[test]
    fn lineup_status_json_shape_is_stable() {
        let json =
            serde_json::to_string_pretty(&LineupStatus::default()).expect("serialize status");
        insta::assert_snapshot!(json);
    }

    #[test]
    fn device_xml_is_stable() {
        let xml = render_device_xml("http", "example.test:8080", fixture_uuid());
        insta::assert_snapshot!(xml);
    }

    #[test]
    fn lineup_item_shape_is_stable() {
        let mut channel = Channel::new(Uuid::nil());
        channel.number = "5".into();
        channel.name = "Concert Classics".into();
        let item = LineupItem::from_channel("http", "example.test:8080", &channel);
        let json = serde_json::to_string_pretty(&item).expect("serialize item");
        insta::assert_snapshot!(json);
    }
}
