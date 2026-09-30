#[cfg(test)]
mod tests {
    use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_app::ApplicationError;
    use ersatzrs_core::repositories::{LocalLibraryAdmin, LocalLibraryPathAdmin};

    async fn fixture() -> AppState {
        let pool = ersatzrs_infra::db::init("sqlite::memory:").await.unwrap();
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        AppState::new(
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
        )
    }

    fn form(id: i64, kind: i32, paths: &[String]) -> LocalLibraryAdmin {
        LocalLibraryAdmin {
            id,
            name: "Synthetic library".into(),
            media_kind: kind,
            paths: paths
                .iter()
                .map(|path| LocalLibraryPathAdmin {
                    id: 0,
                    path: path.clone(),
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn local_library_paths_reject_before_creating_any_rows() {
        let state = fixture().await;
        let root = tempfile::tempdir().unwrap();
        for path in [
            String::new(),
            " ".into(),
            ".".into(),
            "./".into(),
            format!("file://{}", root.path().display()),
            format!("\u{a0}{}", root.path().display()),
            format!("\"{}\"", root.path().display()),
        ] {
            let error = state
                .application
                .save_local_library(form(0, 1, std::slice::from_ref(&path)))
                .await
                .unwrap_err();
            assert!(
                matches!(error, ApplicationError::Validation { field: "library_path_absolute", ref message } if message == &path)
            );
            let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "MediaSource""#)
                .fetch_one(&state.pool)
                .await
                .unwrap();
            assert_eq!(count, 0);
        }
        let missing = root.path().join("missing").display().to_string();
        assert!(
            matches!(state.application.save_local_library(form(0, 1, std::slice::from_ref(&missing))).await.unwrap_err(),
        ApplicationError::Validation { field: "library_path_exists", message } if message == missing)
        );
    }

    #[tokio::test]
    async fn local_library_paths_keep_literal_percent_names_and_repeated_save_identity() {
        let state = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let literal = root.path().join("100%20Movies");
        let spaces = root.path().join("100 Movies");
        std::fs::create_dir(&literal).unwrap();
        std::fs::create_dir(&spaces).unwrap();
        let paths = vec![literal.display().to_string(), spaces.display().to_string()];
        let id = state
            .application
            .save_local_library(form(0, 1, &paths))
            .await
            .unwrap();
        let rows: Vec<(i64, String)> = sqlx::query_as(
            r#"SELECT "Id", "Path" FROM "LibraryPath" WHERE "LibraryId"=? ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&state.pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1, paths[0]);
        assert_eq!(rows[1].1, paths[1]);
        sqlx::query(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (167, ?, 0)"#,
        )
        .bind(rows[0].0)
        .execute(&state.pool)
        .await
        .unwrap();
        for _ in 0..2 {
            // An equivalent spelling must keep the cascade parent and its children.
            let variants = vec![
                literal.join(".").display().to_string(),
                paths[0].clone(),
                paths[1].clone(),
            ];
            state
                .application
                .save_local_library(form(id, 1, &variants))
                .await
                .unwrap();
            let after: Vec<(i64, String)> = sqlx::query_as(
                r#"SELECT "Id", "Path" FROM "LibraryPath" WHERE "LibraryId"=? ORDER BY "Id""#,
            )
            .bind(id)
            .fetch_all(&state.pool)
            .await
            .unwrap();
            assert_eq!(after, rows);
            let parent: i64 =
                sqlx::query_scalar(r#"SELECT "LibraryPathId" FROM "MediaItem" WHERE "Id"=167"#)
                    .fetch_one(&state.pool)
                    .await
                    .unwrap();
            assert_eq!(parent, rows[0].0);
        }
    }

    #[tokio::test]
    async fn local_library_paths_conflicts_preserve_saved_state_and_allow_image_video_pair() {
        let state = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("tv");
        let child = parent.join("kids");
        let sibling = root.path().join("tv2");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::create_dir(&sibling).unwrap();
        let parent = parent.display().to_string();
        let child = child.display().to_string();
        let sibling = sibling.display().to_string();
        state
            .application
            .save_local_library(form(0, 4, &[parent]))
            .await
            .unwrap();
        let id = state
            .application
            .save_local_library(form(0, 1, std::slice::from_ref(&sibling)))
            .await
            .unwrap();
        let before = state
            .application
            .get_local_library(id)
            .await
            .unwrap()
            .unwrap();
        for candidate in [0, id] {
            let mut rejected = form(candidate, 1, std::slice::from_ref(&child));
            rejected.name = "Rejected rename".into();
            let error = state
                .application
                .save_local_library(rejected)
                .await
                .unwrap_err();
            assert!(
                matches!(error, ApplicationError::Validation { field: "library_path_conflict", message } if message == child)
            );
        }
        let after = state
            .application
            .get_local_library(id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before.paths[0].id, after.paths[0].id);
        assert_eq!(after.paths[0].path, sibling);
        assert_eq!(after.name, before.name);
        state
            .application
            .save_local_library(form(0, 6, &[child]))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn local_library_paths_legacy_invalid_values_do_not_block_other_saves() {
        let state = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let id = state
            .application
            .save_local_library(form(0, 1, &[first.display().to_string()]))
            .await
            .unwrap();
        for path in ["relative/legacy", "", "\u{a0}/legacy"] {
            sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
                .bind(id)
                .bind(path)
                .execute(&state.pool)
                .await
                .unwrap();
        }
        state
            .application
            .save_local_library(form(0, 1, &[second.display().to_string()]))
            .await
            .unwrap();
        state
            .application
            .save_local_library(form(id, 1, &[first.display().to_string()]))
            .await
            .unwrap();
        let paths = state
            .application
            .get_local_library(id)
            .await
            .unwrap()
            .unwrap()
            .paths;
        assert_eq!(paths.len(), 1);
    }

    #[tokio::test]
    async fn local_library_paths_failed_insert_rolls_back_name_and_path_removal() {
        let state = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        let replacement = root.path().join("replacement");
        std::fs::create_dir(&old).unwrap();
        std::fs::create_dir(&replacement).unwrap();
        let id = state
            .application
            .save_local_library(form(0, 1, &[old.display().to_string()]))
            .await
            .unwrap();
        let before = state
            .application
            .get_local_library(id)
            .await
            .unwrap()
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (167, ?, 0)"#,
        )
        .bind(before.paths[0].id)
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            r#"CREATE TRIGGER "SyntheticPathFailure" BEFORE INSERT ON "LibraryPath"
        BEGIN SELECT RAISE(ABORT, 'synthetic path failure'); END"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        let mut rejected = form(id, 1, &[replacement.display().to_string()]);
        rejected.name = "Rejected rename".into();
        assert!(state
            .application
            .save_local_library(rejected)
            .await
            .is_err());
        assert_eq!(
            state
                .application
                .get_local_library(id)
                .await
                .unwrap()
                .unwrap(),
            before
        );
        let parent: i64 =
            sqlx::query_scalar(r#"SELECT "LibraryPathId" FROM "MediaItem" WHERE "Id"=167"#)
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_eq!(parent, before.paths[0].id);
    }

    #[tokio::test]
    async fn local_library_paths_correct_equivalent_legacy_rows_without_losing_media() {
        let state = fixture().await;
        let absolute = std::env::current_dir().unwrap().display().to_string();
        let id = state
            .application
            .save_local_library(form(0, 1, std::slice::from_ref(&absolute)))
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "LibraryPath" SET "Path"='.' WHERE "LibraryId"=?"#)
            .bind(id)
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, './')"#)
            .bind(id)
            .execute(&state.pool)
            .await
            .unwrap();
        let paths: Vec<i64> = sqlx::query_scalar(
            r#"SELECT "Id" FROM "LibraryPath" WHERE "LibraryId"=? ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&state.pool)
        .await
        .unwrap();
        for path_id in &paths {
            sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
                .bind(path_id)
                .execute(&state.pool)
                .await
                .unwrap();
        }
        for _ in 0..2 {
            state
                .application
                .save_local_library(form(id, 1, std::slice::from_ref(&absolute)))
                .await
                .unwrap();
            let saved = state
                .application
                .get_local_library(id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(saved.paths.len(), 2);
            assert!(saved.paths.iter().all(|path| path.path == absolute));
            let parents: Vec<i64> = sqlx::query_scalar(
                r#"SELECT "LibraryPathId" FROM "MediaItem" ORDER BY "LibraryPathId""#,
            )
            .fetch_all(&state.pool)
            .await
            .unwrap();
            assert_eq!(parents, paths);
        }
    }

    #[tokio::test]
    async fn local_library_paths_concurrent_creates_cannot_claim_the_same_root() {
        let state = fixture().await;
        let root = tempfile::tempdir().unwrap();
        let paths = vec![root.path().display().to_string()];
        let (one, two) = tokio::join!(
            state.application.save_local_library(form(0, 1, &paths)),
            state.application.save_local_library(form(0, 1, &paths)),
        );
        assert_ne!(one.is_ok(), two.is_ok());
        let error = one.err().or_else(|| two.err()).unwrap();
        assert!(matches!(
            error,
            ApplicationError::Validation {
                field: "library_path_conflict",
                ..
            }
        ));
        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryPath""#)
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn local_library_paths_html_error_retains_draft_and_escapes_path() {
        let state = fixture().await;
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('pages.language', 'nl') ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#)
        .execute(&state.pool)
        .await
        .unwrap();
        let input = serde_json::from_value(serde_json::json!({
            "id": 0, "name": "Draft & library", "media_kind": 1,
            "paths_text": "</textarea><script>alert(1)</script>"
        }))
        .unwrap();
        let response = crate::api::media_sources::save_local_library_form(
            axum::extract::State(state),
            axum::Form(input),
        )
        .await;
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), 100_000)
            .await
            .unwrap();
        let html = std::str::from_utf8(&body).unwrap();
        assert!(html.contains("moet een absoluut pad zijn"));
        assert!(html.contains("Draft &amp; library"));
        assert!(html.contains("&lt;/textarea&gt;&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("name=\"paths_text\""));
        assert!(html.contains("action=\"/admin-api/media-sources/local/save\""));
    }

    #[test]
    fn local_library_paths_feedback_is_localized_for_every_culture() {
        use crate::admin::{i18n::UiLanguage, server_fns::media_sources::local_library_error_text};
        for field in [
            "library_path_absolute",
            "library_path_exists",
            "library_path_conflict",
        ] {
            let error = ApplicationError::Validation {
                field,
                message: "synthetic-path".into(),
            };
            let english = local_library_error_text(UiLanguage::English, &error);
            for code in [
                "de-de", "es-419", "es-es", "fr-fr", "it-it", "nl", "pl", "pt-br",
            ] {
                let translated = local_library_error_text(UiLanguage::from_config(code), &error);
                assert!(translated.contains("synthetic-path"));
                assert_ne!(translated, english, "{code}: {field}");
            }
        }
    }
}
