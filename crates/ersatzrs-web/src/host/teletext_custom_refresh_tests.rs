use super::*;
use ersatzrs_domain::{
    TeletextCustomPageWire, TeletextElementBinding, TELETEXT_CUSTOM_PAGES_SCHEMA,
};

fn page(number: u16, subcode: u16, source: TeletextElementBindingSource) -> TeletextCustomPageWire {
    let mut cells = [b' '; 40];
    cells[..5].copy_from_slice(b"OLD!!");
    cells[20..25].copy_from_slice(b"FIXED");
    TeletextCustomPageWire {
        element_id: 1,
        element_revision: 7,
        page: number,
        subcode,
        national_option: 0,
        rows: vec![TeletextCustomRowWire::new(1, &cells)],
        dynamic_fields: vec![TeletextElementBinding {
            field: "frozen".to_owned(),
            subpage: 0,
            row: 1,
            column: 0,
            length: 5,
            source,
            static_text: String::new(),
        }],
    }
}
fn document() -> TeletextCustomPagesWire {
    TeletextCustomPagesWire {
        schema: TELETEXT_CUSTOM_PAGES_SCHEMA.to_owned(),
        generation: "published".to_owned(),
        pages: vec![
            page(150, 0, TeletextElementBindingSource::NowTitle),
            page(150, 1, TeletextElementBindingSource::Time),
            page(151, 0, TeletextElementBindingSource::Time),
            page(152, 0, TeletextElementBindingSource::Date),
        ],
    }
}
fn clock() -> FieldValues {
    FieldValues {
        date: "15-11-2026".to_owned(),
        time: "00:01".to_owned(),
        ..Default::default()
    }
}
fn cells(document: &TeletextCustomPagesWire, index: usize) -> [u8; 40] {
    document.pages[index].rows[0].decode().unwrap()
}

#[test]
fn failed_programme_page_stays_blank_across_clock_ticks_and_recovers_all_subpages() {
    let original = document();
    let all = CustomPageRefreshTriggers {
        programme: true,
        date: true,
        time: true,
    };
    let (failed, failures) = derive_pages(&original, &[], all, None, &clock()).unwrap();
    assert_eq!(failures, [150]);
    assert_eq!(&cells(&failed, 0)[..5], b"     ");
    assert_eq!(&cells(&failed, 1)[..5], b"     ");
    assert_eq!(&cells(&failed, 2)[..5], b"00:01");
    assert_eq!(&cells(&failed, 3)[..5], b"15-11");
    let later_clock = FieldValues {
        time: "00:02".to_owned(),
        ..clock()
    };
    let (ticked, failures) = derive_pages(
        &failed,
        &failures,
        CustomPageRefreshTriggers {
            time: true,
            ..Default::default()
        },
        None,
        &later_clock,
    )
    .unwrap();
    assert_eq!(failures, [150]);
    assert_eq!(&cells(&ticked, 0)[..5], b"     ");
    assert_eq!(&cells(&ticked, 1)[..5], b"     ");
    assert_eq!(&cells(&ticked, 2)[..5], b"00:02");
    let snapshot = FieldSnapshot {
        values: FieldValues {
            now_title: "Fresh".to_owned(),
            ..Default::default()
        },
        next_programme_transition: None,
    };
    let (recovered, failures) = derive_pages(
        &ticked,
        &failures,
        CustomPageRefreshTriggers {
            programme: true,
            ..Default::default()
        },
        Some(&snapshot),
        &later_clock,
    )
    .unwrap();
    assert!(failures.is_empty());
    assert_eq!(&cells(&recovered, 0)[..5], b"Fresh");
    assert_eq!(&cells(&recovered, 1)[..5], b"00:02");
    for (before, after) in original.pages.iter().zip(&recovered.pages) {
        assert_eq!(after.element_revision, 7);
        assert_eq!(before.dynamic_fields, after.dynamic_fields);
        assert_eq!(&after.rows[0].decode().unwrap()[20..25], b"FIXED");
    }
}

#[test]
fn programme_reconciliation_keeps_clock_cells_until_their_own_boundary() {
    let original = document();
    let snapshot = FieldSnapshot {
        values: FieldValues {
            now_title: "Fresh".to_owned(),
            ..clock()
        },
        next_programme_transition: None,
    };
    let (refreshed, failures) = derive_pages(
        &original,
        &[],
        CustomPageRefreshTriggers {
            programme: true,
            ..Default::default()
        },
        Some(&snapshot),
        &clock(),
    )
    .unwrap();
    assert!(failures.is_empty());
    assert_eq!(&cells(&refreshed, 0)[..5], b"Fresh");
    assert_eq!(&refreshed.pages[1..], &original.pages[1..]);
}

async fn application() -> crate::host::state::AppState {
    use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_core::locking::EntityLocker;
    use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus};
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let (sender, _requests) = tokio::sync::mpsc::unbounded_channel();
    AppState::new(
        pool,
        DatabaseBackend::Sqlite,
        SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        },
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        uuid::Uuid::nil(),
        2,
        IptvAccessTokenVerifier::disabled(),
        sender,
        EntityLocker::new(),
    )
}

#[tokio::test]
async fn actual_owner_read_failure_encodes_blank_mixed_page_and_live_clock_page() {
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    let state = application().await;
    let policy = super::super::teletext_runtime_policy::parse_native_teletext_runtime_policy(
        include_str!("../../tests/fixtures/teletext-output-policy-v1.json"),
    )
    .unwrap()
    .encoding;
    let original = document();
    let (draft, overlays) =
        TeletextCustomPagesDocument::from_wire(original.clone(), &original.generation)
            .unwrap()
            .into_parts();
    let declared = super::super::teletext_documents::lift_base(
        draft,
        policy.v1_region,
        policy.validation.service,
        policy.validation.declarations,
    )
    .unwrap()
    .with_physical_base_overlays(overlays, policy.validation.service)
    .unwrap();
    let deadline = Instant::now().checked_add(Duration::from_secs(30)).unwrap();
    let encoded = Arc::new(
        encode_teletext_authored_service_before(declared, &[], &policy.encoding, deadline).unwrap(),
    );
    let initial = PublishedCustomRefresh {
        document: original,
        transmissions: Vec::new(),
        service: Arc::clone(&encoded),
        clock: FieldValues::default(),
        next_projection: None,
        next_wake: Instant::now(),
        revision: 0,
        failed_pages: Vec::new(),
    };
    let locker = EntityLocker::new();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    // Channel 999 is absent: use the actual repository projection failure path.
    let refreshed = initial
        .refresh_held(
            RefreshContext {
                application: &state.application,
                channel: 999,
                policy,
                deadline,
                poll: Duration::from_secs(5),
            },
            guard.hold_work(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(refreshed.failed_pages, [150]);
    assert_eq!(refreshed.revision, 1);
    assert!(!Arc::ptr_eq(&encoded, refreshed.service()));
    assert_eq!(&cells(&refreshed.document, 0)[..5], b"     ");
    assert_eq!(&cells(&refreshed.document, 1)[..5], b"     ");
    assert_eq!(
        &cells(&refreshed.document, 2)[..5],
        refreshed.clock.time.as_bytes()
    );
    let native = refreshed
        .service()
        .declared()
        .semantic()
        .physical_base_overlays();
    for page in &refreshed.document.pages {
        let overlay = native
            .values()
            .find(|overlay| {
                overlay.owner().page.get() == page.page
                    && overlay.owner().subcode.get() == page.subcode
            })
            .unwrap();
        assert_eq!(overlay.rows()[0].cells(), &page.rows[0].decode().unwrap());
        let units = refreshed
            .service()
            .pages()
            .iter()
            .find(|encoded| {
                encoded.inventory_record()
                    == ersatzrs_ffmpeg::teletext::TeletextPhysicalPageInventoryRecord::Display(
                        overlay.owner(),
                    )
            })
            .unwrap()
            .data_units();
        assert!(units
            .iter()
            .any(|unit| std::array::from_fn::<_, 40, _>(
                |index| unit[6 + index].reverse_bits() & 0x7f
            ) == *overlay.rows()[0].cells()));
    }
    assert!(refreshed.next_wake > Instant::now());
    assert!(refreshed.next_wake <= Instant::now().checked_add(Duration::from_secs(5)).unwrap());
    drop(guard);
    assert_eq!(locker.active_teletext_work(), 0);
}
