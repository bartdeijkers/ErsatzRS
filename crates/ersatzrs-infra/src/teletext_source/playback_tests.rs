use chrono::Utc;
use ersatzrs_core::repositories::{
    ManagedLibraryOperationClaim, ManagedLibraryOperationRepository, ManagedLibraryScanSpec,
};
use ersatzrs_domain::{
    ManagedLibraryOperationKind, SourceTeletextBindingIdentity, SourceTeletextDescriptors,
    SourceTeletextHeaderProgram, SourceTeletextHeaderWindow, SourceTeletextObservedHeader,
    SourceTeletextPrograms, SourceTeletextProvenance, SourceTeletextRevision,
};
use sqlx::{sqlite::SqlitePoolOptions, Executor as _};

use super::*;
use crate::{
    db::begin_write,
    repositories::SqliteManagedLibraryOperationRepository,
    teletext_source::{replace, SourceTeletextParents},
};

struct Fixture {
    pool: DbPool,
    claim: ManagedLibraryOperationClaim,
}

impl Fixture {
    async fn new(provider: MediaSourceKind) -> Self {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute("PRAGMA foreign_keys=ON").await.unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        sqlx::raw_sql(r#"
            INSERT INTO "MediaSource" ("Id") VALUES (11);
            INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId") VALUES (21,'Synthetic',1,11);
            INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (31,21,'fixture');
            INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (101,31);
            INSERT INTO "Movie" ("Id") VALUES (101);
            INSERT INTO "MediaVersion" ("Id", "MovieId", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind")
                VALUES (301,101,'2026-01-01 00:00:00','2026-01-01 00:00:00','00:01:00',576,720,0),
                       (302,101,'2026-01-01 00:00:00','2026-01-01 00:00:00','00:01:00',576,720,0);
            INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash") VALUES
                (501,301,'fixture/one.ts','one'), (502,301,'fixture/two.ts','two'), (503,302,'fixture/other.ts','other');
            INSERT INTO "MediaStream" ("Id", "MediaVersionId", "Index", "Codec", "MediaStreamKind", "Language", "Channels", "Default", "Forced") VALUES
                (401,301,4,'dvb_teletext',3,'eng,nld',0,0,1),
                (402,301,1,'aac',2,'nld',2,1,0), (403,301,2,'subrip',3,'eng',0,1,0);
        "#).execute(&pool).await.unwrap();
        let subtypes = match provider {
            MediaSourceKind::Local => {
                r#"INSERT INTO "LocalLibrary" ("Id") VALUES (21); INSERT INTO "LocalMediaSource" ("Id") VALUES (11);"#
            }
            MediaSourceKind::Plex => {
                r#"INSERT INTO "PlexLibrary" ("Id","ShouldSyncItems") VALUES (21,1); INSERT INTO "PlexMediaSource" ("Id") VALUES (11);"#
            }
            _ => panic!("fixture supports only selected test providers"),
        };
        sqlx::raw_sql(subtypes).execute(&pool).await.unwrap();
        let repository = SqliteManagedLibraryOperationRepository(pool.clone());
        let operation = repository
            .enqueue_scan(
                ManagedLibraryOperationKind::ManualScan,
                21,
                ManagedLibraryScanSpec {
                    provider,
                    show_id: None,
                    deep_scan: true,
                },
                false,
            )
            .await
            .unwrap()
            .unwrap();
        let claim = repository
            .claim_operation(operation.id, Utc::now())
            .await
            .unwrap()
            .unwrap();
        Self { pool, claim }
    }

    async fn store(&self, report: &SourceTeletextObservation) {
        let mut tx = begin_write(&self.pool).await.unwrap();
        assert!(replace(
            &mut tx,
            &self.claim,
            SourceTeletextParents {
                media_version_id: 301,
                media_stream_id: 401,
                media_file_id: 501,
            },
            report
        )
        .await
        .unwrap());
        tx.commit().await.unwrap();
    }

    async fn candidates(&self) -> Result<Vec<SourceTeletextCaptionCandidate>> {
        load_caption_candidates(&self.pool, 101, 301, 501, "fixture/one.ts").await
    }
}

fn descriptors(tag: u8, entries: &[(&str, u8, u8, u8)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for group in entries.chunks(51) {
        bytes.extend([tag, u8::try_from(group.len() * 5).unwrap()]);
        for &(language, role, magazine, page) in group {
            bytes.extend_from_slice(language.as_bytes());
            bytes.extend([(role << 3) | magazine, page]);
        }
    }
    bytes
}

fn report(provider: MediaSourceKind) -> SourceTeletextObservation {
    let mut value = SourceTeletextObservation::new(
        SourceTeletextBinding::Bound(
            SourceTeletextBindingIdentity::new(
                11,
                provider,
                301,
                501,
                401,
                SourceTeletextRevision::new("synthetic-source-revision".into()).unwrap(),
            )
            .unwrap(),
        ),
        4,
        SourceTeletextProvenance::Ffprobe9PackedDescriptorProjection,
        SourceTeletextPid::Reported(258),
        SourceTeletextPrograms::Unavailable,
        SourceTeletextDescriptors::Unavailable,
    )
    .unwrap();
    let mut wire = descriptors(
        0x56,
        &[
            ("eng", 1, 2, 0),
            ("eng", 2, 0, 0x88),
            ("nld", 5, 0, 0x89),
            ("eng", 2, 0, 0x99),
            ("eng", 3, 2, 1),
        ],
    );
    wire.extend(descriptors(0x46, &[("eng", 2, 0, 0x88)]));
    let headers = [(2, 0, 4), (0, 0x88, 0), (0, 0x89, 4), (2, 1, 2)]
        .into_iter()
        .enumerate()
        .map(
            |(ordinal, (wire_magazine, wire_page, controls))| SourceTeletextObservedHeader {
                wire_magazine,
                wire_page,
                subcode: 0,
                controls,
                subtitle_data_unit: false,
                field_parity: false,
                line_offset: 7,
                corrected_address_bytes: 0,
                corrected_header_bytes: 0,
                completion_packet: u32::try_from(ordinal * 10 + 9).unwrap(),
                pes_ordinal: u32::try_from(ordinal).unwrap(),
                unit_ordinal: 0,
                pts: None,
            },
        )
        .collect();
    value.observed_pages =
        SourceTeletextObservedPages::HeadersObserved(Box::new(SourceTeletextHeaderWindow {
            transport_stream_id: 1,
            pat_version: 0,
            psi_fingerprint: [7; 32],
            packet_count: 40,
            complete_pes: 4,
            unobserved_prefix: false,
            ended_at_eof: true,
            programs: vec![SourceTeletextHeaderProgram {
                program_number: 1,
                pmt_pid: 4096,
                pcr_pid: 256,
                pmt_version: 0,
                stream_type: 6,
                descriptors: wire,
            }],
            headers,
        }));
    value
}

fn window(report: &mut SourceTeletextObservation) -> &mut SourceTeletextHeaderWindow {
    let SourceTeletextObservedPages::HeadersObserved(window) = &mut report.observed_pages else {
        panic!("fixture has actual observed headers")
    };
    window
}

#[tokio::test]
async fn candidates_keep_exact_page_language_role_and_repeated_wire_provenance() {
    let fixture = Fixture::new(MediaSourceKind::Local).await;
    let report = report(MediaSourceKind::Local);
    fixture.store(&report).await;
    let before: Vec<(i64, i64, String, i64, i64)> = sqlx::query_as(
        r#"SELECT "Id","Index","Codec","Default","Forced" FROM "MediaStream" ORDER BY "Id""#,
    )
    .fetch_all(&fixture.pool)
    .await
    .unwrap();
    let choices = fixture.candidates().await.unwrap();
    assert_eq!(choices.len(), 2);
    assert_eq!(
        choices
            .iter()
            .map(|choice| (
                choice.request.page.get(),
                choice.language.as_str(),
                choice.descriptor_type,
                choice.is_sdh(),
            ))
            .collect::<Vec<_>>(),
        [(888, "eng", 2, false), (889, "nld", 5, true)]
    );
    let caption = &choices[0];
    assert_eq!(caption.request.stream_index, 4);
    assert_eq!(caption.request.pid, 258);
    assert_eq!(caption.request.program_number, 1);
    assert_eq!(caption.psi_fingerprint, [7; 32]);
    assert_eq!(caption.request.binding.media_version_id, 301);
    assert_eq!(caption.request.binding.media_file_id, 501);
    assert_eq!(caption.request.binding.media_stream_id, 401);
    assert_eq!(
        caption
            .descriptor_occurrences
            .iter()
            .map(|entry| (
                entry.descriptor_tag,
                entry.descriptor_ordinal,
                entry.entry_ordinal
            ))
            .collect::<Vec<_>>(),
        [(0x56, 0, 1), (0x46, 1, 0)]
    );
    // Caption 888 has neither C6 nor a subtitle data-unit marker; guide 200 has
    // C6 but no subtitle descriptor role. The descriptor determines eligibility.
    let after = sqlx::query_as::<_, (i64, i64, String, i64, i64)>(
        r#"SELECT "Id","Index","Codec","Default","Forced" FROM "MediaStream" ORDER BY "Id""#,
    )
    .fetch_all(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(after, before);
    // read-only transaction: prove candidate folding did not rewrite raw reports.
    let mut tx = fixture.pool.begin().await.unwrap();
    assert_eq!(load(&mut tx, 401, 501).await.unwrap(), Some(report));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn candidates_never_fall_back_to_another_item_version_file_or_locator() {
    let fixture = Fixture::new(MediaSourceKind::Local).await;
    fixture.store(&report(MediaSourceKind::Local)).await;
    for (item, version, file, path) in [
        (0, 301, 501, "fixture/one.ts"),
        (101, 0, 501, "fixture/one.ts"),
        (101, 301, 0, "fixture/one.ts"),
        (999, 301, 501, "fixture/one.ts"),
        (101, 302, 501, "fixture/one.ts"),
        (101, 301, 503, "fixture/one.ts"),
        (101, 301, 501, "FIXTURE/one.ts"),
        (101, 301, 501, "private-input-must-not-appear"),
    ] {
        let error = load_caption_candidates(&fixture.pool, item, version, file, path)
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("private-input-must-not-appear"));
    }
    assert!(
        load_caption_candidates(&fixture.pool, 101, 301, 502, "fixture/two.ts")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        load_caption_candidates(&fixture.pool, 101, 302, 503, "fixture/other.ts")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn ambiguous_equal_version_owners_and_stale_source_parents_are_rejected() {
    let fixture = Fixture::new(MediaSourceKind::Local).await;
    fixture.store(&report(MediaSourceKind::Local)).await;
    sqlx::raw_sql(
        r#"INSERT INTO "Song" ("Id") VALUES (101);
        UPDATE "MediaVersion" SET "SongId"=101 WHERE "Id"=301;"#,
    )
    .execute(&fixture.pool)
    .await
    .unwrap();
    assert!(fixture.candidates().await.is_err());
    sqlx::query(r#"UPDATE "MediaVersion" SET "SongId"=NULL WHERE "Id"=301"#)
        .execute(&fixture.pool)
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO "MediaSource" ("Id") VALUES (12);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (12);
        UPDATE "Library" SET "MediaSourceId"=12 WHERE "Id"=21;"#,
    )
    .execute(&fixture.pool)
    .await
    .unwrap();
    assert!(fixture.candidates().await.is_err());
}

#[tokio::test]
async fn duplicate_global_indexes_are_rejected_even_without_a_second_report() {
    for codec in ["dvb_teletext", "aac"] {
        let fixture = Fixture::new(MediaSourceKind::Local).await;
        fixture.store(&report(MediaSourceKind::Local)).await;
        sqlx::query(r#"INSERT INTO "MediaStream" ("Id","MediaVersionId","Index","Codec","MediaStreamKind","Channels","Default","Forced")
            VALUES (405,301,4,?,?,0,0,0)"#).bind(codec).bind(if codec == "aac" { 2 } else { 3 })
            .execute(&fixture.pool).await.unwrap();
        assert!(fixture.candidates().await.is_err());
    }
}

#[tokio::test]
async fn absent_unbound_nonlocal_and_headerless_reports_provide_no_caption_readiness() {
    let fixture = Fixture::new(MediaSourceKind::Local).await;
    assert!(fixture.candidates().await.unwrap().is_empty());
    let mut observation = report(MediaSourceKind::Local);
    observation.observed_pages = SourceTeletextObservedPages::NotAcquired;
    fixture.store(&observation).await;
    assert!(fixture.candidates().await.unwrap().is_empty());
    observation.binding = SourceTeletextBinding::Unavailable;
    fixture.store(&observation).await;
    assert!(fixture.candidates().await.unwrap().is_empty());
    let remote = Fixture::new(MediaSourceKind::Plex).await;
    remote.store(&report(MediaSourceKind::Plex)).await;
    assert!(remote.candidates().await.unwrap().is_empty());
}

#[tokio::test]
async fn one_pid_keeps_distinct_programs_languages_and_roles_instead_of_first_service() {
    let fixture = Fixture::new(MediaSourceKind::Local).await;
    let mut report = report(MediaSourceKind::Local);
    let window = window(&mut report);
    window.programs.push(SourceTeletextHeaderProgram {
        program_number: 2,
        pmt_pid: 4097,
        pcr_pid: 256,
        pmt_version: 0,
        stream_type: 6,
        descriptors: descriptors(
            0x56,
            &[
                ("eng", 2, 0, 0x88),
                ("nld", 2, 0, 0x88),
                ("eng", 5, 0, 0x88),
            ],
        ),
    });
    fixture.store(&report).await;
    let choices = fixture.candidates().await.unwrap();
    assert_eq!(
        choices
            .iter()
            .map(|choice| (
                choice.request.program_number,
                choice.request.page.get(),
                choice.language.as_str(),
                choice.descriptor_type,
            ))
            .collect::<Vec<_>>(),
        [
            (1, 888, "eng", 2),
            (1, 889, "nld", 5),
            (2, 888, "eng", 2),
            (2, 888, "nld", 2),
            (2, 888, "eng", 5)
        ]
    );
}

#[tokio::test]
async fn malformed_wire_descriptors_and_ambiguous_program_identity_never_return_partial_choices() {
    let fixture = Fixture::new(MediaSourceKind::Local).await;
    let mut report = report(MediaSourceKind::Local);
    let mut malformed = window(&mut report).programs[0].clone();
    malformed.program_number = 2;
    malformed.pmt_pid = 4097;
    malformed.descriptors = vec![0x56, 5, b'e'];
    window(&mut report).programs.push(malformed);
    fixture.store(&report).await;
    assert!(fixture.candidates().await.is_err());
    let mut duplicate = window(&mut report).programs[0].clone();
    duplicate.pmt_pid = 4097;
    window(&mut report).programs[1] = duplicate;
    fixture.store(&report).await;
    assert!(fixture.candidates().await.is_err());
}

#[tokio::test]
async fn aggregate_wire_limit_counts_repeated_entries_before_folding_candidates() {
    let fixture = Fixture::new(MediaSourceKind::Local).await;
    let mut report = report(MediaSourceKind::Local);
    let model = window(&mut report).programs[0].clone();
    window(&mut report).programs = (1..=6)
        .map(|program| {
            let count = if program == 6 { 169 } else { 171 };
            SourceTeletextHeaderProgram {
                program_number: program,
                pmt_pid: 4095 + program,
                descriptors: descriptors(0x56, &vec![("eng", 2, 0, 0x88); count]),
                ..model.clone()
            }
        })
        .collect();
    fixture.store(&report).await;
    let choices = fixture.candidates().await.unwrap();
    assert_eq!(choices.len(), 6);
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.descriptor_occurrences.len())
            .sum::<usize>(),
        1024
    );
    window(&mut report).programs[5].descriptors =
        descriptors(0x56, &vec![("eng", 2, 0, 0x88); 170]);
    fixture.store(&report).await;
    assert!(fixture.candidates().await.is_err());
}

#[tokio::test]
async fn exact_stream_bound_succeeds_but_next_row_rejects_the_whole_result() {
    let fixture = Fixture::new(MediaSourceKind::Local).await;
    fixture.store(&report(MediaSourceKind::Local)).await;
    sqlx::raw_sql(r#"WITH RECURSIVE sequence(n) AS (
        SELECT 1 UNION ALL SELECT n+1 FROM sequence WHERE n<255
    ) INSERT INTO "MediaStream" ("Id","MediaVersionId","Index","Codec","MediaStreamKind","Channels","Default","Forced")
    SELECT 1000+n,301,1000+n,'dvb_teletext',3,0,0,0 FROM sequence;"#)
        .execute(&fixture.pool).await.unwrap();
    assert_eq!(fixture.candidates().await.unwrap().len(), 2);
    sqlx::query(r#"INSERT INTO "MediaStream" ("Id","MediaVersionId","Index","Codec","MediaStreamKind","Channels","Default","Forced")
        VALUES (1256,301,1256,'dvb_teletext',3,0,0,0)"#)
        .execute(&fixture.pool).await.unwrap();
    assert!(fixture.candidates().await.is_err());
}
