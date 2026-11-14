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

fn policy() -> TeletextChannelDocumentEncodingPolicy {
    super::super::teletext_runtime_policy::parse_native_teletext_runtime_policy(include_str!(
        "../../tests/fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap()
    .encoding
}

fn initial_source(
    document: TeletextCustomPagesWire,
    policy: &TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> PublishedCustomRefresh {
    let (draft, overlays) =
        TeletextCustomPagesDocument::from_wire(document.clone(), &document.generation)
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
    PublishedCustomRefresh::from_published(
        document,
        Vec::new(),
        Arc::new(
            encode_teletext_authored_service_before(declared, &[], &policy.encoding, deadline)
                .unwrap(),
        ),
    )
}

fn context<'a>(
    state: &'a crate::host::state::AppState,
    policy: &TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> RefreshContext<'a> {
    RefreshContext {
        application: &state.application,
        channel: 999,
        policy: policy.clone(),
        deadline,
        poll: Duration::from_secs(5),
    }
}

fn deadline() -> Instant {
    Instant::now().checked_add(Duration::from_secs(30)).unwrap()
}

fn local_time(day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    use chrono::TimeZone as _;
    chrono::Local
        .with_ymd_and_hms(2026, 11, day, hour, minute, second)
        .single()
        .unwrap()
        .with_timezone(&Utc)
}

fn assert_native_rows(owner: &PublishedCustomRefresh) {
    let native = owner
        .service()
        .declared()
        .semantic()
        .physical_base_overlays();
    for page in &owner.document.pages {
        let overlay = native
            .values()
            .find(|overlay| {
                overlay.owner().page.get() == page.page
                    && overlay.owner().subcode.get() == page.subcode
            })
            .unwrap();
        assert_eq!(overlay.rows()[0].cells(), &page.rows[0].decode().unwrap());
        let units = owner
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
}

#[tokio::test]
async fn actual_owner_read_failure_encodes_blank_mixed_page_and_live_clock_page() {
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    let state = application().await;
    let policy = policy();
    let deadline = deadline();
    let initial = initial_source(document(), &policy, deadline);
    let encoded = Arc::clone(initial.service());
    let locker = EntityLocker::new();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let captured = Utc::now();
    // Channel999 is absent: use the actual repository projection failure path.
    let refreshed = initial
        .refresh_held_with_clock(
            context(&state, &policy, deadline),
            guard.hold_work(),
            true,
            || captured,
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
    assert_native_rows(&refreshed);
    assert!(refreshed.next_wake > Instant::now());
    assert!(refreshed.next_wake <= Instant::now().checked_add(Duration::from_secs(5)).unwrap());
    drop(guard);
    assert_eq!(locker.active_teletext_work(), 0);
}

#[tokio::test]
async fn native_clock_rows_change_at_local_midnight_once_per_minute_and_reconstruct() {
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    let state = application().await;
    let policy = policy();
    let deadline = deadline();
    let mut original = document();
    original.pages.retain(|page| page.page >= 151);
    let initial = initial_source(original.clone(), &policy, deadline);
    let locker = EntityLocker::new();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let before = local_time(14, 23, 59, 20);
    let first = initial
        .refresh_held_with_clock(
            context(&state, &policy, deadline),
            guard.hold_work(),
            true,
            || before,
        )
        .await
        .unwrap();
    assert_eq!(&cells(&first.document, 0)[..5], b"23:59");
    assert_eq!(&cells(&first.document, 1)[..5], b"14-11");
    assert_native_rows(&first);
    let same_minute = local_time(14, 23, 59, 45);
    let unchanged = first
        .refresh_held_with_clock(
            context(&state, &policy, deadline),
            guard.hold_work(),
            false,
            || same_minute,
        )
        .await
        .unwrap();
    assert_eq!(first.revision, unchanged.revision);
    assert!(Arc::ptr_eq(first.service(), unchanged.service()));
    let midnight = local_time(15, 0, 0, 0);
    assert!(!first.current_at(midnight));
    let next = unchanged
        .refresh_held_with_clock(
            context(&state, &policy, deadline),
            guard.hold_work(),
            false,
            || midnight,
        )
        .await
        .unwrap();
    assert_eq!(&cells(&next.document, 0)[..5], b"00:00");
    assert_eq!(&cells(&next.document, 1)[..5], b"15-11");
    assert_eq!(next.revision, first.revision + 1);
    assert!(next.current_at(midnight));
    assert_native_rows(&next);
    let reconstructed = initial_source(original, &policy, deadline)
        .refresh_held_with_clock(
            context(&state, &policy, deadline),
            guard.hold_work(),
            true,
            || midnight,
        )
        .await
        .unwrap();
    assert_eq!(reconstructed.document, next.document);
    assert_native_rows(&reconstructed);
}

#[tokio::test]
async fn unbound_clock_categories_and_failed_blank_fields_do_not_revoke_valid_rows() {
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let state = application().await;
    let policy = policy();
    let deadline = deadline();
    let locker = EntityLocker::new();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let mut date_only = document();
    date_only.pages.retain(|page| page.page == 152);
    let before = local_time(14, 23, 58, 59);
    let after = local_time(14, 23, 59, 0);
    let calls = AtomicUsize::new(0);
    let dated = initial_source(date_only, &policy, deadline)
        .refresh_held_with_clock(
            context(&state, &policy, deadline),
            guard.hold_work(),
            true,
            || {
                if calls.fetch_add(1, Ordering::Relaxed) == 0 {
                    before
                } else {
                    after
                }
            },
        )
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(dated.current_at(after));
    assert_native_rows(&dated);
    let mut mixed = document();
    mixed.pages.retain(|page| page.page == 150);
    let mut fixed = page(153, 0, TeletextElementBindingSource::Date);
    fixed.dynamic_fields.clear();
    mixed.pages.push(fixed.clone());
    let before = local_time(14, 23, 59, 59);
    let after = local_time(15, 0, 0, 0);
    let calls = AtomicUsize::new(0);
    let blank = initial_source(mixed, &policy, deadline)
        .refresh_held_with_clock(
            context(&state, &policy, deadline),
            guard.hold_work(),
            true,
            || {
                if calls.fetch_add(1, Ordering::Relaxed) == 0 {
                    before
                } else {
                    after
                }
            },
        )
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert_eq!(blank.failed_pages, [150]);
    assert!(blank.current_at(after));
    assert_eq!(&cells(&blank.document, 0)[..5], b"     ");
    assert_eq!(&cells(&blank.document, 1)[..5], b"     ");
    assert_eq!(blank.document.pages[2], fixed);
    assert_native_rows(&blank);
}

#[tokio::test]
async fn clock_or_retained_programme_boundary_crossed_during_encoding_refuses_the_result() {
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let state = application().await;
    let policy = policy();
    let deadline = deadline();
    let locker = EntityLocker::new();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let mut original = document();
    original.pages.retain(|page| page.page >= 151);
    let initial = initial_source(original, &policy, deadline);
    let before = local_time(14, 23, 59, 59);
    let after = local_time(15, 0, 0, 0);
    let calls = AtomicUsize::new(0);
    let result = initial
        .refresh_held_with_clock(
            context(&state, &policy, deadline),
            guard.hold_work(),
            true,
            || {
                if calls.fetch_add(1, Ordering::Relaxed) == 0 {
                    before
                } else {
                    after
                }
            },
        )
        .await;
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(result.is_err());
    assert_eq!(initial.revision, 0);
    let captured = after;
    let transition = captured
        .checked_add_signed(chrono::Duration::milliseconds(500))
        .unwrap();
    let finished = captured
        .checked_add_signed(chrono::Duration::seconds(1))
        .unwrap();
    let mut retained = initial_source(document(), &policy, deadline);
    retained.clock = FieldValues::clock(before);
    retained.next_projection = Some(transition);
    retained.programme_transition = Some(transition);
    let calls = AtomicUsize::new(0);
    let result = retained
        .refresh_held_with_clock(
            context(&state, &policy, deadline),
            guard.hold_work(),
            false,
            || {
                if calls.fetch_add(1, Ordering::Relaxed) == 0 {
                    captured
                } else {
                    finished
                }
            },
        )
        .await;
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(result.is_err());
    assert!(!retained.current_at(finished));
}

#[tokio::test]
async fn a_test_utc_clock_cannot_renew_the_real_original_operation_deadline() {
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    let state = application().await;
    let policy = policy();
    let mut original = document();
    original.pages.retain(|page| page.page >= 151);
    let initial = initial_source(original, &policy, deadline());
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    let locker = EntityLocker::new();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let result = initial
        .refresh_held_with_clock(
            context(&state, &policy, expired),
            guard.hold_work(),
            true,
            || local_time(14, 0, 0, 0),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(initial.revision, 0);
    drop(guard);
    assert_eq!(locker.active_teletext_work(), 0);
}
