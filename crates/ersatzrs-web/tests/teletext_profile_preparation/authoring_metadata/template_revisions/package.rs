//! Real immutable packages use the same application installer as the admin.
use super::*;
use std::{
    io::{Cursor, Write as _},
    path::PathBuf,
};

const SCRIPT: &str = r#"case "$1" in
check) printf '%s' '{"status":"ready","code":"ready","message":"ok"}'; exit 0;;
pages) ;;
*) exit 9;;
esac
test "$ERSATZRS_ADDON_CAPABILITY" = teletext.pages.v3 || exit 10
theme="$ERSATZRS_ADDON_SETTING_THEME"
case "$theme" in day|night) ;; *) exit 11;; esac
IFS= read -r input || test -n "$input" || exit 12
generation="${input#*\"generation\":\"}"
generation="${generation%%\"*}"
printf '%s:%s\n' "$ERSATZRS_ADDON_VERSION" "$theme" >> "$ERSATZRS_ADDON_DATA_DIR/template-calls"
printf '%s' "$input" > "$ERSATZRS_ADDON_DATA_DIR/template-input-$theme.json"
sed "s/native-profile-generation/$generation/" "response-$theme.json"
"#;

pub(super) fn response(revision: u8, theme: &str) -> Value {
    let mut value = super::super::response(theme, 123, 16);
    if revision == 2 {
        value["authoring"]["template"]["revision"] = json!("r2");
        value["authoring"]["pages"][0]["regions"][1]["id"] = json!("custom-zone-r2");
        value["authoring"]["pages"][0]["regions"][1]["width"] = json!(18);
        value["authoring"]["pages"][0]["dynamic_fields"][0]["binding"] = json!("guide.subtitle");
    }
    value
}
fn archive(revision: u8) -> Vec<u8> {
    let manifest = format!(
        r#"manifest_version = {ADDON_MANIFEST_VERSION}
id = "{ADDON}"
version = "{revision}.0.0"
name = {{ "en-US" = "Synthetic versioned teletext" }}
summary = {{ "en-US" = "Immutable template revision fixture" }}
license = "GPL-3.0-or-later"
source_url = "https://example.test/source"
host_version = "*"
capabilities = [{{id="addon.check.v1"}},{{id="teletext.pages.v3"}}]
[entrypoints.any]
path = "fixture.sh"
kind = "posix_shell"
[[settings]]
key = "THEME"
label = {{ "en-US" = "Theme" }}
kind = "enum"
required = true
default = "day"
options = ["day","night"]
[[settings]]
key = "RESPONSE"
label = {{ "en-US" = "Response" }}
kind = "enum"
required = true
default = "good"
options = ["good","bad-generation"]
"#
    );
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("addon.toml", manifest.into_bytes()),
        ("fixture.sh", SCRIPT.as_bytes().to_vec()),
        (
            "response-day.json",
            serde_json::to_vec_pretty(&response(revision, "day")).unwrap(),
        ),
        (
            "response-night.json",
            serde_json::to_vec_pretty(&response(revision, "night")).unwrap(),
        ),
    ] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
pub(super) async fn first() -> Fixture {
    let state = PathBuf::from(crate::client_capture::selected("ERSATZRS_APP_STATE_DIR"));
    assert!(state.is_absolute());
    let data = PathBuf::from(crate::client_capture::selected("ERSATZRS_APP_DATA_DIR"));
    assert!(data.is_absolute() && data.starts_with(&state) && data != state);
    if state.exists() {
        assert!(std::fs::read_dir(&state).unwrap().next().is_none());
    } else {
        std::fs::create_dir_all(&state).unwrap();
    }
    // Reuse the persistent fixture without its deliberately unmanaged seed package.
    let root = tempfile::tempdir().unwrap();
    let pool = Fixture::open_persistent_pool(root.path()).await;
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    seed_channels(&pool).await;
    let (state, playout) = Fixture::state_from_pool(pool);
    let fixture = Fixture {
        root,
        state,
        playout,
    };
    fixture
        .state
        .application
        .set_unverified_addon_sources(true)
        .await
        .unwrap();
    update(&fixture, 1).await;
    fixture
        .state
        .application
        .save_addon_setting(ersatzrs_app::AddonSettingEdit {
            addon_id: ADDON.to_owned(),
            key: "THEME".to_owned(),
            value: Some("night".to_owned()),
            reference_kind: None,
            reference: None,
        })
        .await
        .unwrap();
    fixture
        .state
        .application
        .enable_addon(ADDON.to_owned())
        .await
        .unwrap();
    authoring::setting(
        &fixture,
        "teletext.authoring.policy",
        &authoring::policy().to_string(),
    )
    .await;
    authoring::setting(&fixture, MAX_CONCURRENT_SETTING, "1").await;
    authoring::setting(&fixture, "pages.language", "en-us").await;
    fixture
}
pub(super) async fn update(fixture: &Fixture, revision: u8) {
    fixture
        .state
        .application
        .install_addon_from_package(archive(revision))
        .await
        .unwrap();
}
pub(super) async fn installed(fixture: &Fixture) -> ersatzrs_infra::addons::InstalledAddon {
    ersatzrs_infra::addons::load_installed_addon(&fixture.state.pool, ADDON)
        .await
        .unwrap()
        .unwrap()
}
pub(super) fn files(path: &Path) -> Option<Vec<(String, Vec<u8>)>> {
    path.is_dir().then(|| {
        [
            "addon.toml",
            "fixture.sh",
            "response-day.json",
            "response-night.json",
        ]
        .into_iter()
        .map(|name| (name.to_owned(), std::fs::read(path.join(name)).unwrap()))
        .collect()
    })
}
pub(super) async fn calls(fixture: &Fixture) -> Vec<u8> {
    let installed = installed(fixture).await;
    std::fs::read(
        ersatzrs_infra::file_system_layout::addon_data_folder(&installed.data_namespace, ADDON)
            .join("template-calls"),
    )
    .unwrap_or_default()
}
