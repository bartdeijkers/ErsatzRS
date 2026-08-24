use ersatzrs_core::repositories::{AdminSettingsRepository, RepositoryError, SettingsMutation};
use ersatzrs_infra::repositories::SqliteAdminSettingsRepository;

async fn fresh_repository() -> (tempfile::TempDir, SqliteAdminSettingsRepository) {
    let directory = tempfile::tempdir().expect("settings temp directory");
    let database_url = format!(
        "sqlite:{}",
        directory.path().join("admin-settings.db").display()
    );
    let pool = ersatzrs_infra::db::init(&database_url)
        .await
        .expect("initialize settings database");
    (directory, SqliteAdminSettingsRepository(pool))
}

#[tokio::test]
async fn mutation_batch_rolls_back_when_one_write_fails() {
    let (_directory, repository) = fresh_repository().await;
    sqlx::query(
        r#"CREATE TRIGGER "TR_ConfigElement_RejectTestKey"
           BEFORE INSERT ON "ConfigElement"
           WHEN NEW."Key" = 'test.fail'
           BEGIN
               SELECT RAISE(ABORT, 'rejected test key');
           END"#,
    )
    .execute(&repository.0)
    .await
    .expect("create rejection trigger");

    let result = repository
        .apply_mutations(vec![
            SettingsMutation::Set {
                key: "test.first".to_owned(),
                value: "persist only on commit".to_owned(),
            },
            SettingsMutation::Set {
                key: "test.fail".to_owned(),
                value: "reject".to_owned(),
            },
        ])
        .await;

    assert!(matches!(result, Err(RepositoryError::Unavailable { .. })));
    assert_eq!(
        repository
            .get_optional_string("test.first")
            .await
            .expect("read rolled-back setting"),
        None
    );
}

#[tokio::test]
async fn closed_pool_is_reported_as_repository_unavailable() {
    let (_directory, repository) = fresh_repository().await;
    repository.0.close().await;

    let result = repository
        .apply_mutations(vec![SettingsMutation::Set {
            key: "test.closed".to_owned(),
            value: "value".to_owned(),
        }])
        .await;

    assert!(matches!(result, Err(RepositoryError::Unavailable { .. })));
}
