#[cfg(test)]
mod tests {
    use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use chrono::{Duration, TimeZone, Utc};
    use ersatzrs_app::commands::{CreatePlayout, SavePlayoutScheduleFile};
    use ersatzrs_core::{
        repositories::{ChannelRepository, FfmpegProfileRepository},
        scheduling::{
            PlayoutBuildMode, PlayoutSchedulingEngine, SchedulingReferenceData, SchedulingRequest,
            ScriptedSchedulingEngine,
        },
    };
    use ersatzrs_domain::{Channel, FFmpegProfile, Playout};

    async fn fixture() -> (
        AppState,
        tokio::sync::mpsc::UnboundedReceiver<ersatzrs_core::scheduling::PlayoutBuildRequest>,
    ) {
        let pool = ersatzrs_infra::db::init("sqlite::memory:").await.unwrap();
        sqlx::query(r#"INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Synthetic',1920,1080)"#)
            .execute(&pool).await.unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            ersatzrs_infra::db::DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: ersatzrs_infra::db::DatabaseSchemaStatus::default(),
            },
            ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
            uuid::Uuid::nil(),
            2,
            IptvAccessTokenVerifier::disabled(),
            tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );
        state
            .repositories
            .ffmpeg_profile
            .upsert(&FFmpegProfile {
                id: 163,
                name: "Synthetic".into(),
                resolution_id: 1,
                ..Default::default()
            })
            .await
            .unwrap();
        state
            .repositories
            .channel
            .upsert(&Channel {
                id: 163,
                number: "163".into(),
                name: "Synthetic".into(),
                ffmpeg_profile_id: 163,
                ..Default::default()
            })
            .await
            .unwrap();
        (state, rx)
    }

    fn create(kind: &str, value: &str) -> CreatePlayout {
        CreatePlayout {
            kind: kind.into(),
            channel_id: 163,
            program_schedule_id: None,
            schedule_file: value.into(),
        }
    }

    #[tokio::test]
    async fn schedule_file_validation_rejects_creation_without_writes_or_builds() {
        let (state, mut builds) = fixture().await;
        let root = tempfile::tempdir().unwrap();
        for kind in ["scripted", "sequential", "externaljson"] {
            for value in ["", " \t\n", "\"\"", root.path().to_str().unwrap()] {
                let error = state
                    .application
                    .create_playout(create(kind, value))
                    .await
                    .unwrap_err();
                assert!(
                    matches!(
                        error,
                        ersatzrs_app::ApplicationError::Validation {
                            field: "schedule_file",
                            ..
                        }
                    ),
                    "{kind}: {error}"
                );
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Playout""#)
                        .fetch_one(&state.pool)
                        .await
                        .unwrap(),
                    0
                );
                assert!(builds.try_recv().is_err());
            }
        }
    }

    #[tokio::test]
    async fn schedule_file_validation_preserves_saved_script_and_reset_queue() {
        let (state, mut builds) = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("schedule.lua");
        std::fs::write(&file, "wait_until('13:00')").unwrap();
        let original = file.to_str().unwrap();
        let id = state
            .application
            .create_playout(create("scripted", original))
            .await
            .unwrap();
        assert_eq!(builds.try_recv().unwrap().playout_id, id);
        for value in ["", " \t", "\"\" ignored", root.path().to_str().unwrap()] {
            let error = state
                .application
                .save_playout_schedule_file(SavePlayoutScheduleFile {
                    id,
                    schedule_kind: 4,
                    schedule_file: value.into(),
                    rebuild_mode: Some(PlayoutBuildMode::Reset),
                })
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                ersatzrs_app::ApplicationError::Validation {
                    field: "schedule_file",
                    ..
                }
            ));
            let stored: String =
                sqlx::query_scalar(r#"SELECT "ScheduleFile" FROM "Playout" WHERE "Id" = ?"#)
                    .bind(id)
                    .fetch_one(&state.pool)
                    .await
                    .unwrap();
            assert_eq!(stored, original);
            assert!(builds.try_recv().is_err());
        }
    }

    fn request(command: &str) -> SchedulingRequest {
        let start = Utc.with_ymd_and_hms(2026, 9, 30, 12, 0, 0).unwrap();
        SchedulingRequest {
            playout: Playout {
                id: 163,
                schedule_file: Some(command.into()),
                ..Default::default()
            },
            start,
            finish: start + Duration::hours(2),
            mode: PlayoutBuildMode::Reset,
            reference_data: SchedulingReferenceData::default(),
        }
    }

    #[tokio::test]
    async fn schedule_file_validation_executes_accepted_quoted_path_and_arguments() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("synthetic schedule.lua");
        std::fs::write(&file, "assert(arg[0]:match('synthetic schedule.lua$')); assert(arg[1] == '13:15'); assert(arg[2] == 'two words'); assert(arg[3] == ''); wait_until(arg[1])").unwrap();
        for quote in ['\"', '\''] {
            let (state, mut builds) = fixture().await;
            let command = format!(
                "{quote}{}{quote} 13:15 {quote}two words{quote} {quote}{quote}",
                file.display()
            );
            let id = state
                .application
                .create_playout(create("scripted", &command))
                .await
                .unwrap();
            assert_eq!(builds.try_recv().unwrap().playout_id, id);
            let stored: String =
                sqlx::query_scalar(r#"SELECT "ScheduleFile" FROM "Playout" WHERE "Id" = ?"#)
                    .bind(id)
                    .fetch_one(&state.pool)
                    .await
                    .unwrap();
            let result = ScriptedSchedulingEngine.schedule(request(&stored)).unwrap();
            assert_eq!(
                result.anchor.unwrap().next_start,
                Utc.with_ymd_and_hms(2026, 9, 30, 13, 15, 0).unwrap()
            );
        }
    }

    #[test]
    fn schedule_file_validation_empty_build_and_invalid_wait_fail_safely() {
        for value in ["", " \t", "\"\" ignored"] {
            assert!(ScriptedSchedulingEngine.schedule(request(value)).is_err());
        }
        for value in ["", "invalid", "24:00", "12:60", "12:00:60"] {
            let script = format!("wait_until('{value}')");
            assert!(ScriptedSchedulingEngine
                .schedule_script(&script, request(""))
                .is_err());
            let script = format!("local ok = pcall(wait_until, '{value}'); assert(not ok)");
            let initial = request("");
            let start = initial.start;
            let result = ScriptedSchedulingEngine
                .schedule_script(&script, initial)
                .unwrap();
            assert_eq!(result.anchor.unwrap().next_start, start);
        }
    }
}
