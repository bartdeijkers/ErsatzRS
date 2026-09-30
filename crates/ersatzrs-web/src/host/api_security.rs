//! Machine API policy. Browser management and streaming policies are separate
//! boundaries; see ADR 0103 and upstream `Startup.IsApiPath`.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse as _, Response},
};
use ersatzrs_infra::api_security::ApiKey;

const API_KEY_HEADER: &str = "x-ers-api-key";

pub(crate) async fn openapi() -> axum::Json<serde_json::Value> {
    let mut document = serde_json::json!({
        "openapi": "3.1.0",
        "info": {"title": "ErsatzRS machine API", "version": env!("CARGO_PKG_VERSION")},
        "components": {"securitySchemes": {"ApiKey": {
            "type": "apiKey", "in": "header", "name": "X-Ers-Api-Key",
            "description": "API key from api-secrets.json in the application state directory"
        }}, "schemas": {"HlsSessionModel": {
            "type": "object",
            "required": ["channelNumber", "state", "transcodedUntil", "lastAccess"],
            "properties": {
                "channelNumber": {"type": "string"},
                "state": {"type": "string", "description": "Native worker state or next"},
                "transcodedUntil": {"type": ["null", "string"], "format": "date-time",
                    "description": "Host transcode cursor; null for Next or before native startup"},
                "lastAccess": {"type": "string", "format": "date-time"}
            }
        }}},
        "security": [{"ApiKey": []}],
        "paths": {
            "/api/sessions": {"get": {
                "operationId": "getSessions", "summary": "List live playlist-worker sessions",
                "responses": {
                    "200": {"description": "Current native and Next workers",
                        "content": {"application/json": {"schema": {"type": "array",
                            "items": {"$ref": "#/components/schemas/HlsSessionModel"}}}}},
                    "401": {"description": "Missing or invalid API key"},
                    "503": {"description": "Session reporting unavailable"}
                }
            }},
            "/api/maintenance/clean_artwork": {"post": {
                "operationId": "cleanArtwork",
                "summary": "Queue orphan artwork cleanup",
                "parameters": [{"name": "limit", "in": "query", "required": false,
                    "schema": {"type": "integer", "format": "int32", "default": 100_000}}],
                "responses": {
                    "200": {"description": "Cleanup accepted for background execution"},
                    "400": {"description": "Invalid limit"},
                    "401": {"description": "Missing or invalid API key"},
                    "503": {"description": "Cleanup queue unavailable"}
                }
            }},
            "/api/openapi.json": {"get": {
                "operationId": "getMachineApiDescription",
                "summary": "Read this API description",
                "responses": {
                    "200": {"description": "OpenAPI document"},
                    "401": {"description": "Missing or invalid API key"}
                }
            }}
        }
    });
    describe_machine_mutations(&mut document);
    axum::Json(document)
}

fn describe_machine_mutations(document: &mut serde_json::Value) {
    use serde_json::json;
    for (path, method, operation, schema, codes) in [
        (
            "/api/ffmpeg/profiles",
            "get",
            "GetFFmpegProfiles",
            None,
            &[500][..],
        ),
        (
            "/api/ffmpeg/profiles/new",
            "post",
            "CreateFFmpegProfile",
            Some("FfmpegProfileMutation"),
            &[400, 409, 500][..],
        ),
        (
            "/api/ffmpeg/profiles/update",
            "put",
            "UpdateFFmpegProfile",
            Some("FfmpegProfileMutation"),
            &[400, 404, 409, 500][..],
        ),
        (
            "/api/ffmpeg/delete/{id}",
            "delete",
            "DeleteFFmpegProfile",
            None,
            &[400, 404, 409, 500][..],
        ),
        (
            "/api/collections/smart",
            "get",
            "GetSmartCollections",
            None,
            &[500][..],
        ),
        (
            "/api/collections/smart/new",
            "post",
            "CreateSmartCollection",
            Some("SmartCollectionMutation"),
            &[400, 500][..],
        ),
        (
            "/api/collections/smart/update",
            "put",
            "UpdateSmartCollection",
            Some("SmartCollectionMutation"),
            &[400, 404, 500][..],
        ),
        (
            "/api/collections/smart/delete/{id}",
            "delete",
            "DeleteSmartCollection",
            None,
            &[404, 500][..],
        ),
        (
            "/api/maintenance/empty_trash",
            "post",
            "EmptyTrash",
            None,
            &[500][..],
        ),
    ] {
        let mut entry = json!({"operationId":operation,"responses":{
            "200":{"description":"Operation completed"},
            "401":{"description":"Missing or invalid API key"}
        }});
        for code in codes {
            let description = match code {
                400 => "Invalid request or mixed validation errors",
                404 => "Requested record does not exist",
                409 => "Mutation conflicts with current state",
                _ => "Operation failed",
            };
            entry["responses"][code.to_string()] = json!({"description":description});
        }
        if path.contains("{id}") {
            entry["parameters"] = json!([{"name":"id","in":"path","required":true,
                "schema":{"type":"integer","format":"int64"}}]);
        }
        if let Some(schema) = schema {
            entry["requestBody"] = json!({"required":true,"content":{"application/json":{
                "schema":{"$ref":format!("#/components/schemas/{schema}")}
            }}});
        }
        document["paths"][path][method] = entry;
    }
    describe_machine_mutation_schemas(document);
}

fn describe_machine_mutation_schemas(document: &mut serde_json::Value) {
    use serde_json::{json, Value};
    let mut properties = serde_json::Map::new();
    for name in [
        "name",
        "vaapiDisplay",
        "vaapiDevice",
        "videoProfile",
        "videoPreset",
    ] {
        properties.insert(
            name.into(),
            if name == "name" {
                json!({"type":"string","maxLength":50})
            } else {
                json!({"type":["string","null"]})
            },
        );
    }
    for name in [
        "ffmpegProfileId",
        "resolutionId",
        "threadCount",
        "videoBitrate",
        "videoBufferSize",
        "audioBitrate",
        "audioBufferSize",
        "audioChannels",
        "audioSampleRate",
    ] {
        properties.insert(name.into(), json!({"type":"integer"}));
    }
    properties.insert(
        "qsvExtraHardwareFrames".into(),
        json!({"type":["integer","null"]}),
    );
    properties.insert("targetLoudness".into(), json!({"type":["number","null"]}));
    for name in [
        "normalizeAudio",
        "normalizeVideo",
        "allowBFrames",
        "normalizeFramerate",
        "normalizeColors",
        "deinterlaceVideo",
    ] {
        properties.insert(name.into(), json!({"type":"boolean"}));
    }
    add_profile_enum_schemas(&mut properties);
    document["components"]["schemas"]["FfmpegProfileMutation"] = json!({
        "type":"object","required":["name","resolutionId"],"properties":Value::Object(properties),
        "description":"Accepts upstream NormalizeAudio/Video and ErsatzRS TranscodeAudio/Video names. Updates preserve ErsatzRS-only HDR and tune values. ffmpegProfileId selects the update record."
    });
    document["components"]["schemas"]["SmartCollectionMutation"] = json!({"type":"object",
        "required":["name","query"],"properties":{"id":{"type":"integer"},
            "name":{"type":"string","maxLength":50},"query":{"type":"string"}}
    });
}

fn add_profile_enum_schemas(properties: &mut serde_json::Map<String, serde_json::Value>) {
    use serde_json::json;
    for (name, names, numbers) in [
        (
            "hardwareAcceleration",
            vec![
                "None",
                "Qsv",
                "Nvenc",
                "Vaapi",
                "VideoToolbox",
                "Amf",
                "V4l2m2m",
                "Rkmpp",
            ],
            vec![0, 1, 2, 3, 4, 5, 6, 7],
        ),
        (
            "vaapiDriver",
            vec!["Default", "iHD", "i965", "RadeonSI", "Nouveau"],
            vec![0, 1, 2, 3, 4],
        ),
        (
            "scalingBehavior",
            vec!["ScaleAndPad", "Stretch", "Crop"],
            vec![0, 1, 2],
        ),
        (
            "padMode",
            vec!["HardwareIfPossible", "Software"],
            vec![0, 1],
        ),
        (
            "videoFormat",
            vec!["None", "H264", "Hevc", "Mpeg2Video", "Av1", "Copy"],
            vec![0, 1, 2, 3, 4, 99],
        ),
        ("bitDepth", vec!["EightBit", "TenBit"], vec![0, 1]),
        (
            "tonemapAlgorithm",
            vec!["Linear", "Clip", "Gamma", "Reinhard", "Mobius", "Hable"],
            vec![0, 1, 2, 3, 4, 5],
        ),
        (
            "audioFormat",
            vec!["None", "Aac", "Ac3", "AacLatm", "Copy"],
            vec![0, 1, 2, 3, 99],
        ),
        ("normalizeLoudnessMode", vec!["Off", "LoudNorm"], vec![0, 1]),
    ] {
        properties.insert(
            name.into(),
            json!({"oneOf":[{"type":"string","enum":names},{"type":"integer","enum":numbers}]}),
        );
    }
}

pub(crate) async fn authenticate(
    State(key): State<Option<ApiKey>>,
    mut request: Request,
    next: Next,
) -> Response {
    for (name, value) in request.headers_mut().iter_mut() {
        if name == API_KEY_HEADER {
            value.set_sensitive(true);
        }
    }
    let path = request.uri().path();
    if is_segment(path, "/api") && !is_segment(path, "/api/troubleshoot") {
        let mut values = request.headers().get_all(API_KEY_HEADER).iter();
        let supplied = values.next();
        let authorized = values.next().is_none()
            && supplied.is_some_and(|value| {
                key.as_ref()
                    .is_some_and(|key| key.verifies(value.as_bytes()))
            });
        if !authorized {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    next.run(request).await
}

fn is_segment(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|tail| tail.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, middleware, routing::any, Router};
    use tower::ServiceExt as _;

    #[test]
    fn generated_browser_functions_are_registered_outside_machine_namespace() {
        let paths: Vec<_> = leptos::server_fn::axum::server_fn_paths().collect();
        assert!(
            !paths.is_empty(),
            "server-function inventory must be linked"
        );
        for (path, _) in paths {
            assert!(
                path.starts_with("/admin-api/"),
                "unexpected browser function path: {path}"
            );
        }
    }

    fn router(key: Option<ApiKey>) -> Router {
        Router::new()
            .route("/{*path}", any(|| async { StatusCode::NO_CONTENT }))
            .layer(middleware::from_fn_with_state(key, authenticate))
    }

    #[tokio::test]
    async fn machine_namespace_fails_closed_without_configured_key() {
        for path in [
            "/api",
            "/api/maintenance/clean_artwork",
            "/api/unknown",
            "/api/troubleshoot-extra",
        ] {
            for method in ["GET", "POST", "OPTIONS", "DELETE"] {
                let response = router(None)
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(path)
                            .header(API_KEY_HEADER, "synthetic-fixture-key")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }
        }
    }

    #[tokio::test]
    async fn only_one_exact_header_authorizes_machine_requests() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("api-secrets.json"),
            br#"{"ApiKey":"synthetic-fixture-key"}"#,
        )
        .unwrap();
        let key = ersatzrs_infra::api_security::load_or_create(root.path()).unwrap();
        for (headers, expected) in [
            (vec![], StatusCode::UNAUTHORIZED),
            (vec![""], StatusCode::UNAUTHORIZED),
            (vec!["synthetic-wrong-key"], StatusCode::UNAUTHORIZED),
            (vec!["synthetic-fixture-key"], StatusCode::NO_CONTENT),
            (
                vec!["synthetic-fixture-key", "synthetic-fixture-key"],
                StatusCode::UNAUTHORIZED,
            ),
            (
                vec!["synthetic-fixture-key, synthetic-fixture-key"],
                StatusCode::UNAUTHORIZED,
            ),
        ] {
            let mut request =
                Request::post("/api/maintenance/clean_artwork?api_key=synthetic-fixture-key")
                    .header("cookie", "ApiKey=synthetic-fixture-key")
                    .header("host", "localhost")
                    .header("x-forwarded-for", "127.0.0.1");
            for value in headers {
                request = request.header(API_KEY_HEADER, value);
            }
            let response = router(Some(key.clone()))
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
    }

    #[tokio::test]
    async fn management_exception_is_delimited_and_other_surfaces_are_independent() {
        for path in [
            "/api/troubleshoot",
            "/api/troubleshoot/playback/log",
            "/admin-api/save",
            "/iptv/channels.m3u",
            "/apiculture",
        ] {
            let response = router(None)
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
        }
    }

    #[tokio::test]
    async fn production_router_authenticates_before_maintenance_admission() {
        use crate::host::{
            artwork_cleanup::TokioArtworkCleanupQueue,
            state::{AppState, IptvAccessTokenVerifier},
        };
        use std::sync::Arc;

        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("api-secrets.json"),
            br#"{"ApiKey":"synthetic-fixture-key"}"#,
        )
        .unwrap();
        let mut state = AppState::for_tests(IptvAccessTokenVerifier::disabled());
        state.api_key = Some(ersatzrs_infra::api_security::load_or_create(root.path()).unwrap());
        let (queue, mut receiver) = TokioArtworkCleanupQueue::channel();
        state.artwork_cleanup = Arc::new(queue);
        let app = crate::host::router::build_app(state);
        for (header, supplied) in [
            (API_KEY_HEADER, None),
            (API_KEY_HEADER, Some("synthetic-wrong-key")),
            ("x-etv-api-key", Some("synthetic-fixture-key")),
        ] {
            let mut request = Request::post("/api/maintenance/clean_artwork?limit=7");
            if let Some(value) = supplied {
                request = request.header(header, value);
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert!(receiver.try_recv().is_err());
        }
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/maintenance/clean_artwork?limit=7")
                    .header(API_KEY_HEADER, "synthetic-fixture-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(receiver.try_recv().unwrap().limit, 7);
        let response = app
            .clone()
            .oneshot(
                Request::get("/api/openapi.json")
                    .header(API_KEY_HEADER, "synthetic-fixture-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 16_384)
            .await
            .unwrap();
        let description: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            description["components"]["securitySchemes"]["ApiKey"]["name"],
            "X-Ers-Api-Key"
        );
        assert_eq!(description["security"], serde_json::json!([{"ApiKey": []}]));
        for path in [
            "/api/settings/ui/save",
            "/api/unknown",
            "/api/troubleshoot-extra",
        ] {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        let response = app
            .oneshot(
                Request::get("/admin-api/settings/ui/save")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}
