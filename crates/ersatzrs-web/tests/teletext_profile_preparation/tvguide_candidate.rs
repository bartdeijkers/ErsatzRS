//! Real production-package preparation; ordinary publication/client proof is separate.
//! The parent intentionally restricts this host fixture to Unix.

use super::*;
use ersatzrs_core::{
    programme::{ProgrammeOccurrence, ProgrammeSegment},
    xmltv::{XmltvProgramme, XmltvProgrammeContent},
};
use ersatzrs_domain::{TeletextColourPixelOptions, TeletextFlashPhase};
use ersatzrs_ffmpeg::teletext::{
    render_teletext_composition, EncodedTeletextAuthoredPage, TeletextRasterBudget,
};
use ersatzrs_infra::addons::{
    install_addon_from_local_source, install_local_repository_bundle,
    set_unverified_sources_enabled, AddonRuntimeContext,
};
use ersatzrs_web::host::teletext_documents::{
    prepare_bound_teletext_document_candidate, EncodedBoundTeletextDocumentCandidate,
};

const TVGUIDE: &str = "org.ersatzrs.addon.tvguide-teletext";

async fn profile(fixture: &Fixture, name: &str, entries: &[(&str, &str)]) -> TeletextProfile {
    let installation = fixture
        .state
        .application
        .teletext_installation_snapshot(TVGUIDE)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    saved(
        fixture
            .state
            .application
            .save_teletext_profile(&TeletextProfileSave {
                addon_id: TVGUIDE.to_owned(),
                name: name.to_owned(),
                settings: entries
                    .iter()
                    .map(|(key, value)| TeletextProfileSetting {
                        key: (*key).to_owned(),
                        value: TeletextProfileValue::Literal((*value).to_owned()),
                    })
                    .collect(),
                expected_installation: installation,
                existing: None,
            })
            .await
            .unwrap(),
    )
    .profile
}

async fn installed_fixture() -> Fixture {
    let zip = super::client_capture::selected("ERSATZRS_TVGUIDE_CANDIDATE_ZIP");
    for key in ["ERSATZRS_APP_STATE_DIR", "ERSATZRS_APP_DATA_DIR"] {
        let value = super::client_capture::selected(key);
        assert!(
            Path::new(&value).is_absolute(),
            "test directory must be absolute"
        );
    }
    let fixture = Fixture::new_persistent().await;
    set_unverified_sources_enabled(&fixture.state.pool, true)
        .await
        .unwrap();
    let imported =
        install_local_repository_bundle(&fixture.state.pool, std::fs::read(zip).unwrap())
            .await
            .unwrap();
    let installed = install_addon_from_local_source(
        &fixture.state.pool,
        &imported.id,
        TVGUIDE,
        false,
        &AddonRuntimeContext::default(),
    )
    .await
    .unwrap();
    assert!(
        !installed.enabled,
        "installation must remain explicitly disabled"
    );
    assert_eq!(installed.manifest.id, TVGUIDE);
    assert!(installed.manifest.has_capability("teletext.pages.v3"));
    fixture
        .state
        .application
        .enable_addon(TVGUIDE.to_owned())
        .await
        .unwrap();
    fixture
}

fn assert_candidate(
    candidate: &EncodedBoundTeletextDocumentCandidate,
    channel: i64,
    first: u16,
    last: u16,
    index: u16,
    header: &str,
) {
    assert_eq!(candidate.selection().channel_id, channel);
    assert_eq!(candidate.generation(), GENERATION);
    let base = candidate.encoded().declared().semantic().base();
    assert_eq!(base.initial_page.get(), index);
    assert!(base.pages.len() >= 2);
    for page in &base.pages {
        assert!((first..=last).contains(&page.address.page.get()));
        let run = &page.rows.iter().find(|row| row.row == 1).unwrap().runs[0];
        assert_eq!(
            run.content,
            TeletextRunContent::Text {
                text: header.to_owned()
            }
        );
    }
    for page in candidate.encoded().pages() {
        let composition = match page {
            EncodedTeletextAuthoredPage::NativeDisplay(page) => page.composition(),
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => page.normalized().original(),
            _ => panic!("base guide needs no optional authored object/DRCS bank"),
        };
        let raster = render_teletext_composition(
            composition,
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            TeletextRasterBudget {
                max_pixels: 168_000,
                max_diagnostics: 1400,
            },
        )
        .unwrap();
        assert!(raster.diagnostics().is_empty());
        assert!(!page.data_units().is_empty());
    }
}

#[tokio::test]
#[ignore = "requires an explicit production bundle and isolated state/data directories"]
async fn production_tvguide_profiles_install_validate_and_encode_without_publication() {
    let fixture = installed_fixture().await;
    let day = profile(&fixture, "Day", &[("HEADER", "DAY")]).await;
    let night = profile(
        &fixture,
        "Night",
        &[
            ("PAGE_START", "400"),
            ("PAGE_END", "499"),
            ("INDEX_PAGE", "410"),
            ("THEME", "night"),
            ("HEADER", "NIGHT"),
        ],
    )
    .await;
    let day_binding = fixture.bind(1, &day, None).await;
    let night_binding = fixture.bind(2, &night, None).await;
    let start = Utc.with_ymd_and_hms(2026, 3, 28, 11, 55, 0).unwrap();
    let finish = start + chrono::Duration::minutes(35);
    let programmes = [ProgrammeOccurrence {
        playout_id: 1,
        first_item_id: 1,
        programme: XmltvProgramme {
            channel_number: "1".to_owned(),
            start,
            stop: finish,
            title: "Café programme".to_owned(),
            custom_title: None,
            content: XmltvProgrammeContent::default(),
        },
        segments: vec![ProgrammeSegment {
            item_id: 1,
            media_item_id: Some(1),
            start,
            finish,
            in_point: chrono::Duration::zero(),
            out_point: finish - start,
        }],
    }];
    let channels = [TeletextGuideChannel {
        id: 1,
        number: "1",
        name: "Synthetic channel",
        programmes: &programmes,
    }];
    let context = guide(&channels);
    let application = &fixture.state.application;
    // Respect the existing single-worker admission policy; retain both results.
    // Simultaneous ordinary-channel delivery remains a separate P9B-02 trial.
    let day_candidate = prepare_bound_teletext_document_candidate(
        application,
        1,
        TVGUIDE,
        day_binding.revision,
        &context,
        documents::policy(),
    )
    .await;
    let night_candidate = prepare_bound_teletext_document_candidate(
        application,
        2,
        TVGUIDE,
        night_binding.revision,
        &context,
        documents::policy(),
    )
    .await;
    for (candidate, channel, first, last, index, header) in [
        (day_candidate.unwrap(), 1, 200, 299, 200, "DAY"),
        (night_candidate.unwrap(), 2, 400, 499, 410, "NIGHT"),
    ] {
        assert_candidate(&candidate, channel, first, last, index, header);
    }
    let settings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM AddonSetting WHERE AddonId=?")
        .bind(TVGUIDE)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(
        settings, 0,
        "profiles must not mutate installation settings"
    );
    let published: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM TeletextDesignRevision")
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(
        published, 0,
        "preparation must not create or publish designs"
    );
}
