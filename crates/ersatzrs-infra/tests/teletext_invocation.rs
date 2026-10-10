//! Unix synthetic installed-record proof, separate from ZIP/native acceptance.
#![cfg(unix)]

mod common;

use std::{collections::BTreeMap, time::Duration};

use ersatzrs_addon_contract::{
    AddonCapability, AddonEntrypoint, AddonEntrypointKind, AddonManifest, AddonPermissions,
    TeletextPagesResponse, ADDON_MANIFEST_VERSION,
};
use ersatzrs_infra::addons::{
    calculate_addon_content_sha256, enable_addon, invoke_enabled_addon,
    invoke_enabled_addon_with_input, load_installed_addon, resolve_enabled_addon_command,
    teletext_invocation, AddonInvocation, AddonRuntimeContext,
};

const ADDON_ID: &str = "org.ersatzrs.teletext.public-fixture";
const RESPONSE: &str = r#"{"schema_version":1,"generation":"public-fixture",
"language":"nld","initial_page":123,"pages":[{
"address":{"page":123,"subcode":0},"kind":"full_page","language":"nld",
"national_option":0,"rows":[{"row":1,"runs":[{"column":0,"style":{
"foreground":"white","background":"black","height":"normal",
"boxed":false,"flash":false,"conceal":false},"content":{
"kind":"text","text":"Original engine fixture"}}]}],
"navigation":[{"role":"index","target":{"page":123,"subcode":null}}]}]}"#;

fn fixture_manifest() -> AddonManifest {
    AddonManifest {
        manifest_version: ADDON_MANIFEST_VERSION,
        id: ADDON_ID.to_owned(),
        version: semver::Version::new(1, 0, 0),
        name: BTreeMap::from([("en-US".to_owned(), "Engine fixture".to_owned())]),
        summary: BTreeMap::from([("en-US".to_owned(), "Original engine fixture".to_owned())]),
        icon: None,
        license: "GPL-3.0-or-later".to_owned(),
        source_url: "https://example.test/source".to_owned(),
        host_version: semver::VersionReq::STAR,
        capabilities: ["addon.check.v1", "teletext.pages.v1"]
            .map(|id| AddonCapability {
                id: id.to_owned(),
                options: Vec::new(),
            })
            .to_vec(),
        dependencies: Vec::new(),
        entrypoints: BTreeMap::from([(
            "any".to_owned(),
            AddonEntrypoint {
                path: "addon.sh".to_owned(),
                kind: AddonEntrypointKind::PosixShell,
            },
        )]),
        permissions: AddonPermissions::default(),
        settings: Vec::new(),
        media_list_storage: None,
        default_source_hosts: Vec::new(),
        panels: Vec::new(),
    }
}

#[tokio::test]
async fn public_dedicated_invocation_uses_enabled_hashed_package_and_exact_revision() {
    let pool = common::test_db().await;
    let temporary = tempfile::tempdir().unwrap();
    let package = temporary.path().join("package");
    std::fs::create_dir(&package).unwrap();
    // Capture stays outside the immutable package tree, inside this disposable
    // directory. No production data namespace or OS permission sandbox is tested.
    std::fs::write(
        package.join("addon.sh"),
        format!(
            "case \"$1\" in\ncheck) printf '%s' '{{\"status\":\"ready\",\"code\":\"ready\",\"message\":\"ok\"}}';;\npages) test \"$ERSATZRS_ADDON_CAPABILITY\" = teletext.pages.v1 || exit 9; cat > ../observed-request.json; printf '%s' '{RESPONSE}';;\n*) exit 8;;\nesac\n"
        ),
    )
    .unwrap();
    let manifest = fixture_manifest();
    manifest.validate().unwrap();
    let hash = calculate_addon_content_sha256(&package).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonInstallation"
        ("AddonId","Version","Enabled","PackageSha256","ContentSha256",
         "ManifestJson","InstallPath","InstalledAt","UpdatedAt","StateRevision")
        VALUES (?, ?, 0, 'synthetic-package', ?, ?, ?, 'now', 'now', 'before-enable')"#,
    )
    .bind(ADDON_ID)
    .bind(manifest.version.to_string())
    .bind(hash)
    .bind(serde_json::to_string(&manifest).unwrap())
    .bind(package.to_str().unwrap())
    .execute(&pool)
    .await
    .unwrap();
    let runtime = AddonRuntimeContext::default();
    enable_addon(&pool, ADDON_ID, &runtime).await.unwrap();
    let installed = load_installed_addon(&pool, ADDON_ID)
        .await
        .unwrap()
        .unwrap();
    assert!(installed.enabled);
    let input = br#"{"schema_version":1,"generation":"public-fixture","locale":"nl-NL",
"timezone":"Europe/Amsterdam","captured_at":"2026-03-28T12:00:00Z",
"window":{"start":"2026-03-27T23:00:00Z","finish":"2026-03-30T22:00:00Z"},
"channels":[]}"#
        .to_vec();
    let receipt = tokio::time::timeout(
        Duration::from_secs(10),
        teletext_invocation::invoke(
            pool.clone(),
            ADDON_ID.to_owned(),
            runtime.clone(),
            input.clone(),
            None,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(receipt.state_revision, installed.state_revision);
    assert_eq!(receipt.stdout, RESPONSE.as_bytes());
    assert_eq!(
        std::fs::read(temporary.path().join("observed-request.json")).unwrap(),
        input
    );
    let response = TeletextPagesResponse::from_json(&receipt.stdout, "public-fixture").unwrap();
    assert_eq!(response.initial_page, 123);
    assert_eq!(response.pages.len(), 1);
    // Raw wire acceptance is not layout/encoding validation or publication.
    let extra = BTreeMap::new();
    let generic = || AddonInvocation {
        capability: "teletext.pages.v1",
        operation: "pages",
        runtime: &runtime,
        extra_environment: &extra,
        timeout: Duration::from_secs(1),
        max_output_bytes: 1024,
    };
    assert!(
        resolve_enabled_addon_command(&pool, ADDON_ID, "teletext.pages.v1", "pages", &runtime)
            .await
            .is_err()
    );
    assert!(invoke_enabled_addon(&pool, ADDON_ID, generic())
        .await
        .is_err());
    assert!(
        invoke_enabled_addon_with_input(&pool, ADDON_ID, generic(), &input)
            .await
            .is_err()
    );
}
