// The application statically links SQLite through SQLx on every target.
// CVE-2025-6965 affects SQLite releases before 3.50.2.
#[tokio::test]
async fn bundled_sqlite_version_excludes_cve_2025_6965() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("open the bundled SQLite runtime");
    let version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&pool)
        .await
        .expect("read the linked SQLite version");
    let parts: Vec<u32> = version
        .split('.')
        .map(|part| part.parse().expect("numeric SQLite version component"))
        .collect();
    let [major, minor, patch, ..] = parts.as_slice() else {
        panic!("unexpected SQLite version: {version}");
    };
    assert!(
        (*major, *minor, *patch) >= (3, 50, 2),
        "bundled SQLite {version} is older than the CVE-2025-6965 fix"
    );
}
