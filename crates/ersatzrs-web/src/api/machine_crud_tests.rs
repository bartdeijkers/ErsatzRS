#[cfg(test)]
mod tests {
    use crate::host::{
        router,
        state::{AppState, IptvAccessTokenVerifier, SystemStartup},
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};
    use ersatzrs_domain::{Channel, FFmpegProfile};
    use ersatzrs_ffmpeg::FfmpegRuntimeReport;
    use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus};
    use serde_json::{json, Value};
    use sqlx::Executor as _;
    use tokio::sync::mpsc;
    use tower::ServiceExt as _;
    use uuid::Uuid;

    struct Fixture {
        _root: tempfile::TempDir,
        _builds: mpsc::UnboundedReceiver<ersatzrs_core::scheduling::PlayoutBuildRequest>,
        state: AppState,
        app: Router,
    }

    impl Fixture {
        async fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(4)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(root.path().join("fixture.db"))
                        .create_if_missing(true),
                )
                .await
                .unwrap();
            sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
            pool.execute(r#"INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'1920x1080',1920,1080)"#).await.unwrap();
            let (tx, builds) = mpsc::unbounded_channel();
            let mut state = AppState::new(
                pool,
                DatabaseBackend::Sqlite,
                SystemStartup {
                    database_ready: true,
                    schema_status: DatabaseSchemaStatus::default(),
                },
                FfmpegRuntimeReport::for_tests(),
                Uuid::nil(),
                1,
                IptvAccessTokenVerifier::disabled(),
                tx,
                ersatzrs_core::locking::EntityLocker::default(),
            );
            std::fs::write(
                root.path().join("api-secrets.json"),
                br#"{"ApiKey":"synthetic-crud-key"}"#,
            )
            .unwrap();
            state.api_key =
                Some(ersatzrs_infra::api_security::load_or_create(root.path()).unwrap());
            state
                .internal_http_addr
                .set("127.0.0.1:18554".parse().unwrap())
                .unwrap();
            let app = router::build_app(state.clone());
            Self {
                _root: root,
                _builds: builds,
                state,
                app,
            }
        }

        async fn call(&self, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
            call(
                self.app.clone(),
                method,
                path,
                body,
                Some("synthetic-crud-key"),
            )
            .await
        }
    }

    async fn call(
        app: Router,
        method: &str,
        path: &str,
        body: Value,
        key: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .extension(axum::extract::ConnectInfo(
                "127.0.0.1:18554".parse::<std::net::SocketAddr>().unwrap(),
            ))
            .header("content-type", "application/json");
        if let Some(key) = key {
            request = request.header("X-Ers-Api-Key", key);
        }
        let response = app
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        assert!(
            !response.headers().contains_key("location"),
            "machine routes must not redirect to browser forms"
        );
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 32_768)
            .await
            .unwrap();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8(bytes.to_vec()).unwrap()))
        };
        (status, value)
    }

    fn profile(name: &str) -> Value {
        json!({"name":name,"threadCount":2,"resolutionId":1,
        "normalizeAudio":true,"normalizeVideo":true,"hardwareAcceleration":"None",
        "vaapiDriver":"Default","scalingBehavior":"ScaleAndPad","padMode":"Software",
        "videoFormat":"H264","videoProfile":"high","videoPreset":"veryfast","bitDepth":"EightBit",
        "videoBitrate":4000,"videoBufferSize":8000,"tonemapAlgorithm":"Hable",
        "audioFormat":"Aac","audioBitrate":192,"audioBufferSize":384,"audioChannels":2,
        "audioSampleRate":48,"normalizeLoudnessMode":"Off","deinterlaceVideo":true})
    }

    #[tokio::test]
    async fn profile_machine_request_rejects_invalid_name_without_browser_redirect() {
        let fixture = Fixture::new().await;
        assert_eq!(
            fixture
                .call("POST", "/api/ffmpeg/profiles/new", profile(" "))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            fixture
                .state
                .application
                .count_ffmpeg_profiles()
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn profile_machine_crud_has_typed_failures_round_trip_and_no_rejected_writes() {
        let fixture = Fixture::new().await;
        let (status, created) = fixture
            .call("POST", "/api/ffmpeg/profiles/new", profile("First"))
            .await;
        assert_eq!(status, StatusCode::OK);
        let id = created["id"].as_i64().unwrap();
        assert_eq!(created["ffmpegProfileId"], id);
        let (status, rows) = fixture
            .call("GET", "/api/ffmpeg/profiles", Value::Null)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rows[0]["videoFormat"], "H264");
        assert_eq!(rows[0]["audioFormat"], "Aac");
        assert_eq!(rows[0]["vaapiDriver"], "Default");
        assert_eq!(rows[0]["resolution"], "1920x1080");
        assert!(rows[0].get("qsvExtraHardwareFrames").is_none());
        assert_rejected_profile_mutations(&fixture).await;
        let mut missing = profile("Unique");
        missing["ffmpegProfileId"] = json!(999);
        assert_eq!(
            fixture
                .call("PUT", "/api/ffmpeg/profiles/update", missing.clone())
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        missing["name"] = json!("First");
        assert_eq!(
            fixture
                .call("PUT", "/api/ffmpeg/profiles/update", missing)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        let mut updated = profile("Renamed");
        updated["ffmpegProfileId"] = json!(id);
        updated["audioChannels"] = json!(6);
        assert_eq!(
            fixture
                .call("PUT", "/api/ffmpeg/profiles/update", updated)
                .await
                .0,
            StatusCode::OK
        );
        let persisted = fixture
            .state
            .repositories
            .ffmpeg_profile
            .get(id)
            .await
            .unwrap();
        assert_eq!(persisted.name, "Renamed");
        assert_eq!(persisted.audio_channels, 6);
        assert_eq!(
            fixture
                .state
                .application
                .count_ffmpeg_profiles()
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .call("DELETE", "/api/ffmpeg/delete/999", Value::Null)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            fixture
                .call("DELETE", &format!("/api/ffmpeg/delete/{id}"), Value::Null)
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            fixture
                .state
                .application
                .count_ffmpeg_profiles()
                .await
                .unwrap(),
            0
        );
    }

    async fn assert_rejected_profile_mutations(fixture: &Fixture) {
        for (field, bad) in [
            ("name", json!("x".repeat(51))),
            ("threadCount", json!(-1)),
            ("resolutionId", json!(99)),
            ("videoFormat", json!("bad")),
            ("hardwareAcceleration", json!(99)),
        ] {
            let mut request = profile("Rejected");
            request[field] = bad;
            assert_eq!(
                fixture
                    .call("POST", "/api/ffmpeg/profiles/new", request)
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            fixture
                .call("POST", "/api/ffmpeg/profiles/new", profile("First"))
                .await
                .0,
            StatusCode::CONFLICT
        );
        let mut mixed = profile("First");
        mixed["threadCount"] = json!(-1);
        assert_eq!(
            fixture
                .call("POST", "/api/ffmpeg/profiles/new", mixed)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn profile_delete_guards_default_and_channel_playout_graph_in_one_transaction() {
        let fixture = Fixture::new().await;
        fixture
            .state
            .repositories
            .ffmpeg_profile
            .upsert(&FFmpegProfile {
                id: 7,
                name: "Used".into(),
                resolution_id: 1,
                ..Default::default()
            })
            .await
            .unwrap();
        fixture
            .state
            .repositories
            .channel
            .upsert(&Channel {
                id: 1,
                number: "1".into(),
                name: "Synthetic".into(),
                ffmpeg_profile_id: 7,
                ..Default::default()
            })
            .await
            .unwrap();
        fixture.state.pool.execute(r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES(1,1,'Classic',0)"#).await.unwrap();
        assert_eq!(
            fixture
                .call("DELETE", "/api/ffmpeg/delete/7", Value::Null)
                .await
                .0,
            StatusCode::CONFLICT
        );
        fixture.state.pool.execute(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES('ffmpeg.default_profile_id','7')"#).await.unwrap();
        assert_eq!(
            fixture
                .call("DELETE", "/api/ffmpeg/delete/7", Value::Null)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        for (table, query) in [
            ("FFmpegProfile", r#"SELECT COUNT(*) FROM "FFmpegProfile""#),
            ("Channel", r#"SELECT COUNT(*) FROM "Channel""#),
            ("Playout", r#"SELECT COUNT(*) FROM "Playout""#),
        ] {
            let count: i64 = sqlx::query_scalar(query)
                .fetch_one(&fixture.state.pool)
                .await
                .unwrap();
            assert_eq!(count, 1, "rejected deletion preserves {table}");
        }
    }

    #[tokio::test]
    async fn profile_duplicate_race_keeps_one_row_and_rejects_the_other_request() {
        let fixture = Fixture::new().await;
        let (a, b) = tokio::join!(
            fixture.call("POST", "/api/ffmpeg/profiles/new", profile("Concurrent")),
            fixture.call("POST", "/api/ffmpeg/profiles/new", profile("Concurrent"))
        );
        let mut statuses = [a.0.as_u16(), b.0.as_u16()];
        statuses.sort_unstable();
        assert_eq!(statuses, [200, 409]);
        assert_eq!(
            fixture
                .state
                .application
                .count_ffmpeg_profiles()
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn smart_machine_crud_keeps_duplicate_validation_and_missing_record_statuses() {
        let fixture = Fixture::new().await;
        let request = json!({"name":"Picks","query":"title:synthetic"});
        let (status, created) = fixture
            .call("POST", "/api/collections/smart/new", request.clone())
            .await;
        assert_eq!(status, StatusCode::OK);
        let id = created["id"].as_i64().unwrap();
        assert_eq!(
            fixture
                .call("POST", "/api/collections/smart/new", request)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        for (name, query) in [(" ", "title:synthetic"), ("Other", ""), ("Other", "(")] {
            assert_eq!(
                fixture
                    .call(
                        "POST",
                        "/api/collections/smart/new",
                        json!({"name":name,"query":query})
                    )
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            fixture
                .call(
                    "PUT",
                    "/api/collections/smart/update",
                    json!({"id":999,"name":"Other","query":"title:synthetic"})
                )
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            fixture
                .call(
                    "PUT",
                    "/api/collections/smart/update",
                    json!({"id":999,"name":" ","query":"title:synthetic"})
                )
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        let (status, updated) = fixture
            .call(
                "PUT",
                "/api/collections/smart/update",
                json!({"id":id,"name":"Updated","query":"title:updated"}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(updated["smartCollectionId"], id);
        let (status, rows) = fixture
            .call("GET", "/api/collections/smart", Value::Null)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            rows,
            json!([{"id":id,"name":"Updated","query":"title:updated"}])
        );
        assert_eq!(
            fixture
                .call("DELETE", "/api/collections/smart/delete/999", Value::Null)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            fixture
                .call(
                    "DELETE",
                    &format!("/api/collections/smart/delete/{id}"),
                    Value::Null
                )
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            fixture
                .call(
                    "DELETE",
                    &format!("/api/collections/smart/delete/{id}"),
                    Value::Null
                )
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn machine_mutations_authenticate_before_body_or_writes_and_stay_off_streaming_listener()
    {
        let fixture = Fixture::new().await;
        for (method, path) in [
            ("POST", "/api/ffmpeg/profiles/new"),
            ("PUT", "/api/ffmpeg/profiles/update"),
            ("DELETE", "/api/ffmpeg/delete/7"),
            ("POST", "/api/collections/smart/new"),
            ("PUT", "/api/collections/smart/update"),
            ("DELETE", "/api/collections/smart/delete/7"),
            ("POST", "/api/maintenance/empty_trash"),
        ] {
            for key in [None, Some("wrong-fixture-key")] {
                assert_eq!(
                    call(fixture.app.clone(), method, path, Value::Null, key)
                        .await
                        .0,
                    StatusCode::UNAUTHORIZED
                );
            }
            assert_eq!(
                call(
                    router::build_streaming_app(fixture.state.clone()),
                    method,
                    path,
                    Value::Null,
                    Some("synthetic-crud-key")
                )
                .await
                .0,
                StatusCode::NOT_FOUND
            );
        }
        assert_eq!(
            fixture
                .state
                .application
                .count_ffmpeg_profiles()
                .await
                .unwrap(),
            0
        );
        let (status, description) = fixture.call("GET", "/api/openapi.json", Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            description["paths"]["/api/ffmpeg/profiles/new"]["post"]["responses"]
                .get("409")
                .is_some()
        );
        assert!(
            description["paths"]["/api/collections/smart/new"]["post"]["responses"]
                .get("409")
                .is_none()
        );
        assert_eq!(description["security"], json!([{"ApiKey":[]}]));
    }

    #[tokio::test]
    async fn empty_trash_and_concat_keep_safe_operational_failure_statuses() {
        let fixture = Fixture::new().await;
        assert_eq!(
            fixture
                .call("POST", "/api/maintenance/empty_trash", Value::Null)
                .await
                .0,
            StatusCode::OK
        );
        let internal = router::build_internal_app(fixture.state.clone());
        assert_eq!(
            call(
                internal.clone(),
                "GET",
                "/ffmpeg/concat/999",
                Value::Null,
                None
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        fixture.state.pool.close().await;
        let (status, message) = fixture
            .call("POST", "/api/maintenance/empty_trash", Value::Null)
            .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(message, "Operation failed");
        assert_eq!(
            call(internal, "GET", "/ffmpeg/concat/999", Value::Null, None)
                .await
                .0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        for path in ["/api/ffmpeg/profiles", "/api/collections/smart"] {
            let (status, message) = fixture.call("GET", path, Value::Null).await;
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(message, "Operation failed");
        }
    }

    #[tokio::test]
    async fn profile_wire_keeps_numeric_enums_nullable_qsv_and_hdr_tune_updates() {
        let fixture = Fixture::new().await;
        let mut request = profile("QSV nullable");
        request["hardwareAcceleration"] = json!(1);
        request["videoFormat"] = json!(1);
        request["vaapiDriver"] = json!(0);
        request["qsvExtraHardwareFrames"] = Value::Null;
        let (status, created) = fixture
            .call("POST", "/api/ffmpeg/profiles/new", request.clone())
            .await;
        assert_eq!(status, StatusCode::OK);
        let id = created["id"].as_i64().unwrap();
        assert!(fixture
            .state
            .repositories
            .ffmpeg_profile
            .get(id)
            .await
            .unwrap()
            .qsv_extra_hardware_frames
            .is_none());
        request["ffmpegProfileId"] = json!(id);
        request["qsvExtraHardwareFrames"] = json!(48);
        assert_eq!(
            fixture
                .call("PUT", "/api/ffmpeg/profiles/update", request)
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            fixture
                .state
                .repositories
                .ffmpeg_profile
                .get(id)
                .await
                .unwrap()
                .qsv_extra_hardware_frames,
            Some(48)
        );
        fixture
            .state
            .repositories
            .ffmpeg_profile
            .upsert(&FFmpegProfile {
                id: 77,
                name: "HDR".into(),
                resolution_id: 1,
                hardware_acceleration: ersatzrs_domain::HardwareAccelerationKind::Nvenc,
                video_format: ersatzrs_domain::FFmpegProfileVideoFormat::Hevc,
                bit_depth: ersatzrs_domain::FFmpegProfileBitDepth::TenBit,
                video_tune: Some("hq".into()),
                output_dynamic_range: ersatzrs_domain::OutputDynamicRange::Hdr10Bt2020Pq,
                hdr_mastering_display: Some(
                    ersatzrs_domain::ffmpeg::MasteringDisplayMetadata::BT2020_1000_NITS,
                ),
                hdr_content_light: Some(ersatzrs_domain::ffmpeg::ContentLightLevel::DEFAULT),
                ..Default::default()
            })
            .await
            .unwrap();
        let before = fixture
            .state
            .repositories
            .ffmpeg_profile
            .get(77)
            .await
            .unwrap();
        let mut request = profile("HDR renamed");
        request["ffmpegProfileId"] = json!(77);
        request["hardwareAcceleration"] = json!("Nvenc");
        request["videoFormat"] = json!("Hevc");
        request["bitDepth"] = json!("TenBit");
        request["videoPreset"] = json!("p4");
        assert_eq!(
            fixture
                .call("PUT", "/api/ffmpeg/profiles/update", request)
                .await
                .0,
            StatusCode::OK
        );
        let after = fixture
            .state
            .repositories
            .ffmpeg_profile
            .get(77)
            .await
            .unwrap();
        assert_eq!(after.video_tune, before.video_tune);
        assert_eq!(after.output_dynamic_range, before.output_dynamic_range);
        assert_eq!(after.hdr_mastering_display, before.hdr_mastering_display);
        assert_eq!(after.hdr_content_light, before.hdr_content_light);
    }

    #[tokio::test]
    async fn smart_duplicate_race_keeps_bad_request_contract_and_single_persisted_row() {
        let fixture = Fixture::new().await;
        let request = json!({"name":"Concurrent smart","query":"title:synthetic"});
        let (a, b) = tokio::join!(
            fixture.call("POST", "/api/collections/smart/new", request.clone()),
            fixture.call("POST", "/api/collections/smart/new", request)
        );
        let mut statuses = [a.0.as_u16(), b.0.as_u16()];
        statuses.sort_unstable();
        assert_eq!(statuses, [200, 400]);
        assert_eq!(
            fixture
                .call("GET", "/api/collections/smart", Value::Null)
                .await
                .1
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
