use super::*;
use ersatzrs_core::repositories::{ChannelRepository as _, FfmpegProfileRepository as _};
use ersatzrs_domain::{Channel, FFmpegProfile};
use ersatzrs_infra::{
    repositories::make_repositories,
    xmltv::{GuideExpansionLimits, GuideReadLimits},
};
use serde_json::Value;

fn budget() -> GuideReadBudget {
    GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: 100,
            max_source_bytes: 64 * 1024,
        },
        GuideExpansionLimits {
            max_nodes: 1000,
            max_bytes: 1024 * 1024,
        },
    )
}

async fn database() -> (tempfile::TempDir, DbPool) {
    let root = tempfile::tempdir().unwrap();
    let pool = ersatzrs_infra::db::init(&format!(
        "sqlite:{}",
        root.path().join("guide.db").display()
    ))
    .await
    .unwrap();
    for sql in [
        r#"INSERT OR IGNORE INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Synthetic',320,180)"#,
        r#"INSERT OR IGNORE INTO "ConfigElement" ("Key","Value") VALUES ('pages.language','en-us')"#,
        r#"INSERT OR IGNORE INTO "ConfigElement" ("Key","Value") VALUES ('xmltv.time_zone','Local')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    let repositories = make_repositories(pool.clone());
    repositories
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 77,
            name: "Synthetic".into(),
            resolution_id: 1,
            ..FFmpegProfile::default()
        })
        .await
        .unwrap();
    for id in [10, 11] {
        repositories
            .channel
            .upsert(&Channel {
                id,
                unique_id: uuid::Uuid::from_u128(u128::try_from(id).unwrap()),
                number: id.to_string(),
                name: format!("Synthetic {id}"),
                ffmpeg_profile_id: 77,
                is_enabled: true,
                ..Channel::default()
            })
            .await
            .unwrap();
    }
    (root, pool)
}

fn at() -> DateTime<Utc> {
    "2026-10-24T23:30:00.125Z".parse().unwrap()
}

#[test]
fn locale_resolution_uses_all_catalog_codes_and_aliases_without_unknown_fallback() {
    assert_eq!(crate::admin::languages::LANGUAGES.len(), 9);
    for definition in crate::admin::languages::LANGUAGES {
        for raw in std::iter::once(definition.code).chain(definition.aliases.iter().copied()) {
            assert_eq!(resolve_locale(Some(raw)).unwrap(), definition.code);
            assert_eq!(
                resolve_locale(Some(&format!("  {}  ", raw.to_ascii_uppercase()))).unwrap(),
                definition.code
            );
        }
    }
    for raw in [
        None,
        Some(""),
        Some("unknown"),
        Some("UTC"),
        Some("en-us-extra"),
    ] {
        assert!(matches!(
            resolve_locale(raw),
            Err(TeletextGuideCaptureError::Locale)
        ));
    }
    let oversized = "n".repeat(65);
    assert!(matches!(
        resolve_locale(Some(&oversized)),
        Err(TeletextGuideCaptureError::Locale)
    ));
}

#[tokio::test]
async fn captured_locale_channels_and_dst_window_reach_both_request_versions_unchanged() {
    let (_root, pool) = database().await;
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value"=' NL_NL ' WHERE "Key"='pages.language'"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='Utc' WHERE "Key"='xmltv.time_zone'"#)
        .execute(&pool)
        .await
        .unwrap();
    let clock = ProgrammeClock::named("Europe/Amsterdam").unwrap();
    let mut limits = budget();
    let captured = capture_with_clock(
        &pool,
        &[10, 11],
        at(),
        &mut limits,
        Instant::now() + std::time::Duration::from_secs(30),
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(captured.locale(), "nl");
    assert_eq!(captured.timezone(), "Europe/Amsterdam");
    let (start, finish) = clock
        .calendar_day_bounds(at(), NonZeroU8::new(3).unwrap())
        .unwrap();
    assert_eq!((finish - start).num_hours(), 73);
    let mut requests = Vec::new();
    for schema in [1, 2] {
        let mut request: Value = serde_json::from_slice(
            &captured
                .request_bytes("capture-generation", schema)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(request["schema_version"], schema);
        assert_eq!(request["locale"], "nl");
        assert_eq!(request["timezone"], "Europe/Amsterdam");
        assert_eq!(request["channels"][0]["id"], 10);
        assert_eq!(request["channels"][1]["id"], 11);
        assert_eq!(request["channels"][0]["name"], "Synthetic 10");
        assert_eq!(request["channels"][0]["programmes"], serde_json::json!([]));
        assert_eq!(
            request["window"]["start"]
                .as_str()
                .unwrap()
                .parse::<DateTime<Utc>>()
                .unwrap(),
            start
        );
        assert_eq!(
            request["window"]["finish"]
                .as_str()
                .unwrap()
                .parse::<DateTime<Utc>>()
                .unwrap(),
            finish
        );
        request["schema_version"] = Value::Null;
        requests.push(request);
    }
    assert_eq!(requests[0], requests[1]);
    // Later changes cannot alter the already-owned captured request.
    sqlx::query(r#"UPDATE "Channel" SET "Name"='Changed' WHERE "Id"=10"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='en-us' WHERE "Key"='pages.language'"#)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&captured.request_bytes("capture-generation", 2).unwrap())
            .unwrap()["channels"][0]["name"],
        "Synthetic 10"
    );
    assert_eq!(captured.locale(), "nl");
    assert!(!format!("{captured:?}").contains("Synthetic"));
}

#[tokio::test]
async fn missing_unknown_locale_and_original_deadline_fail_without_rewriting_settings() {
    let (_root, pool) = database().await;
    let clock = ProgrammeClock::named("UTC").unwrap();
    for raw in [Some("unsupported"), None] {
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"='pages.language'"#)
            .execute(&pool)
            .await
            .unwrap();
        if let Some(value) = raw {
            sqlx::query(
                r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES ('pages.language',?)"#,
            )
            .bind(value)
            .execute(&pool)
            .await
            .unwrap();
        }
        let mut limits = budget();
        assert!(matches!(
            capture_with_clock(
                &pool,
                &[10],
                at(),
                &mut limits,
                Instant::now() + std::time::Duration::from_secs(30),
                &clock
            )
            .await,
            Err(TeletextGuideCaptureError::Locale)
        ));
        let stored: Option<String> = sqlx::query_scalar(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"='pages.language'"#,
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(stored.as_deref(), raw);
    }
    let mut limits = budget();
    assert!(matches!(
        capture_with_clock(&pool, &[10], at(), &mut limits, Instant::now(), &clock).await,
        Err(TeletextGuideCaptureError::TimedOut)
    ));
    assert_eq!(limits.usage().records, 0);
}

#[tokio::test]
async fn actual_system_clock_identity_is_preserved_or_explicitly_rejected() {
    let (_root, pool) = database().await;
    let mut limits = budget();
    let result = capture_teletext_guide(
        &pool,
        &[10],
        at(),
        &mut limits,
        Instant::now() + std::time::Duration::from_secs(30),
    )
    .await;
    match ProgrammeClock::system()
        .ok()
        .and_then(|clock| clock.iana_name().map(str::to_owned))
    {
        Some(expected) => assert_eq!(result.unwrap().timezone(), expected),
        None => assert!(matches!(result, Err(TeletextGuideCaptureError::Clock))),
    }
}
